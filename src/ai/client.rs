use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::PatchSuggestion;
use crate::context::ProjectContext;
use crate::errors::ErrorFinding;

/// انتزاع بالای هر AI provider، تا بشود بعداً بین Anthropic/OpenAI/local model
/// جابه‌جا شد بدون تغییر بقیه‌ی کدبیس. جز MVP نیست ولی هزینه‌اش الان کمه.
#[async_trait]
pub trait PatchProvider: Send + Sync {
    async fn suggest_patch(
        &self,
        finding: &ErrorFinding,
        context: &ProjectContext,
    ) -> Result<PatchSuggestion>;
}

pub struct AnthropicClient {
    api_key: String,
    http: reqwest::Client,
    model: String,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        let model = std::env::var("JEV_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            http,
            model,
        }
    }

    #[allow(dead_code)]
    pub fn new_with_model(api_key: String, model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            http,
            model,
        }
    }

    fn build_prompt(finding: &ErrorFinding, context: &ProjectContext) -> String {
        let mut prompt = format!(
            "خطای زیر در یک پروژه‌ی {:?} رخ داده است:\n\n{}\n\n",
            finding.toolchain, finding.raw_snippet
        );

        if let Some(diff) = &context.relevant_diff {
            prompt.push_str(&format!(
                "دیف تغییرات commit‌نشده:\n```diff\n{}\n```\n\n",
                diff
            ));
        }

        for (path, content) in &context.relevant_file_snippets {
            prompt.push_str(&format!("محتوای فایل {}:\n```\n{}\n```\n\n", path, content));
        }

        prompt.push_str(
            "لطفاً فقط یک unified diff برای رفع این خطا برگردان، به‌همراه یک توضیح یک‌خطی. \
             فرمت پاسخ باید دقیقاً JSON زیر باشد: \
             {\"explanation\": \"...\", \"unified_diff\": \"...\", \"file_path\": \"...\"}",
        );

        prompt
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct PatchJson {
    explanation: String,
    unified_diff: String,
    file_path: String,
}

#[async_trait]
impl PatchProvider for AnthropicClient {
    async fn suggest_patch(
        &self,
        finding: &ErrorFinding,
        context: &ProjectContext,
    ) -> Result<PatchSuggestion> {
        let prompt = Self::build_prompt(finding, context);

        let req_body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1500,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&req_body)
            .send()
            .await
            .context("ارسال درخواست به Anthropic API ناموفق بود")?
            .error_for_status()
            .context("Anthropic API خطای HTTP برگرداند (کلید/سهمیه را بررسی کنید)")?
            .json::<AnthropicResponse>()
            .await
            .context("پارس پاسخ Anthropic API ناموفق بود")?;

        let text_block = resp
            .content
            .iter()
            .find(|b| b.block_type == "text")
            .and_then(|b| b.text.clone())
            .context("پاسخ متنی از AI دریافت نشد")?;

        // پاک‌سازی احتمالی fence های Markdown اطراف JSON
        let cleaned = text_block
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let parsed: PatchJson =
            serde_json::from_str(cleaned).context("پاسخ AI به فرمت JSON مورد انتظار نبود")?;

        Ok(PatchSuggestion {
            file_path: parsed.file_path,
            unified_diff: parsed.unified_diff,
            explanation: parsed.explanation,
        })
    }
}

/// Provider آفلاین برای دمو/تست بدون کلید API.
/// یک unified diff معتبر می‌سازد تا کل پایپ‌لاین (detect -> collect ->
/// save -> apply) بدون هزینه قابل نمایش باشد.
pub struct MockProvider;

#[async_trait]
impl PatchProvider for MockProvider {
    async fn suggest_patch(
        &self,
        finding: &ErrorFinding,
        context: &ProjectContext,
    ) -> Result<PatchSuggestion> {
        let file_path = finding
            .file_hint
            .clone()
            .or_else(|| {
                context
                    .relevant_file_snippets
                    .first()
                    .map(|(p, _)| p.clone())
            })
            .unwrap_or_else(|| "TODO.jev.md".to_string());

        let original = context
            .relevant_file_snippets
            .iter()
            .find(|(p, _)| p == &file_path)
            .map(|(_, c)| c.clone())
            .unwrap_or_default();

        let explanation = format!(
            "[mock] خطای {:?} شناسایی شد؛ این پچ نمونه است. برای پچ واقعی ANTHROPIC_API_KEY را تنظیم کنید.",
            finding.toolchain
        );

        let unified_diff = if original.is_empty() {
            format!(
                "--- /dev/null\n+++ b/{}\n@@ -0,0 +1,2 @@\n+<!-- jev mock patch: {} -->\n+{}\n",
                file_path,
                explanation,
                finding.raw_snippet.lines().next().unwrap_or("")
            )
        } else {
            let first_line = original.lines().next().unwrap_or("");
            let modified = format!(
                "{}\n{}",
                first_line,
                original.lines().skip(1).collect::<Vec<_>>().join("\n")
            );
            // diff واقعی با diffy تا همیشه قابل apply باشد
            diffy::create_patch(&original, &modified).to_string()
        };

        Ok(PatchSuggestion {
            file_path,
            unified_diff,
            explanation,
        })
    }
}

/// اگر `ANTHROPIC_API_KEY` تنظیم شده باشد client واقعی، وگرنه mock.
/// خروجی دوم می‌گوید از کدام استفاده شد (برای لاگ به کاربر).
pub fn make_provider() -> (Box<dyn PatchProvider>, &'static str) {
    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.trim().is_empty() => (Box::new(AnthropicClient::new(k)), "anthropic"),
        _ => (
            Box::new(MockProvider),
            "mock (ANTHROPIC_API_KEY تنظیم نشده)",
        ),
    }
}
