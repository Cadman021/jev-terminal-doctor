use anyhow::Result;
use git2::Repository;
use std::path::Path;

use super::{project, ProjectContext};
use crate::errors::ErrorFinding;

/// بر اساس یک ErrorFinding (که از errors::detector آمده)، بافت لازم برای
/// ساختن یک پرامپت خوب برای AI را جمع می‌کند: نوع پروژه، دیف گیت فعلی
/// (تغییرات commit‌نشده که احتمالاً باعث خطا شده‌اند) و محتوای فایل خطا.
///
/// سقف‌ها (جلوگیری از انفجار توکن): diff حداکثر 8KB، هر فایل حداکثر
/// 60 خط اطراف `line_hint` و حداکثر 6KB.
pub fn collect(root: &Path, finding: &ErrorFinding) -> Result<ProjectContext> {
    let mut ctx = ProjectContext {
        project_kind: project::detect_project_kind(root).map(|s| s.to_string()),
        ..Default::default()
    };

    // دیف تغییرات commit‌نشده — معمولاً همین چیزی است که خطا را ایجاد کرده
    if let Ok(repo) = Repository::open(root) {
        if let Ok(diff_text) = diff_working_tree(&repo) {
            if !diff_text.trim().is_empty() {
                ctx.relevant_diff = Some(truncate_chars(&diff_text, 8000));
            }
        }
    }

    // محتوای فایلی که کامپایلر/تست‌رانر به آن اشاره کرده (برش اطراف خط)
    if let Some(file_hint) = &finding.file_hint {
        let file_path = root.join(file_hint);
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            let sliced = match finding.line_hint {
                Some(line) => slice_around(&content, line as usize, 60, 6000),
                None => truncate_chars(&content, 6000),
            };
            ctx.relevant_file_snippets.push((file_hint.clone(), sliced));
        }
    }

    Ok(ctx)
}

/// `context_lines` خط قبل و بعد از `line_1based` را برمی‌گرداند.
fn slice_around(
    content: &str,
    line_1based: usize,
    context_lines: usize,
    max_chars: usize,
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let idx = line_1based
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1));
    let start = idx.saturating_sub(context_lines);
    let end = (idx + context_lines + 1).min(lines.len());
    truncate_chars(&lines[start..end].join("\n"), max_chars)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect()
}

fn diff_working_tree(repo: &Repository) -> Result<String> {
    let diff = repo.diff_index_to_workdir(None, None)?;
    let mut out = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        if let Ok(content) = std::str::from_utf8(line.content()) {
            out.push_str(content);
        }
        true
    })?;
    Ok(out)
}
