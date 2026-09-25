use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ai::PatchSuggestion;

// deferred: برای overlay زنده نگه داشته شده؛ allow تا CI سبز بماند.
#[allow(dead_code)]
pub enum UserAction {
    Apply,
    Edit,
    Dismiss,
}

/// کلید فشرده‌شده توسط کاربر را به یک UserAction ترجمه می‌کند.
/// Ctrl+F برای Apply (fix)، Ctrl+E برای Edit، Esc برای Dismiss.
#[allow(dead_code)]
pub fn map_key(key: crossterm::event::KeyEvent) -> Option<UserAction> {
    use crossterm::event::{KeyCode, KeyModifiers};

    match (key.code, key.modifiers) {
        (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(UserAction::Apply),
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => Some(UserAction::Edit),
        (KeyCode::Esc, _) => Some(UserAction::Dismiss),
        _ => None,
    }
}

/// اعمال پچ روی فایل هدف به‌صورت hunk-by-hunk.
///
/// - مسیر `patch.file_path` باید نسبی و داخل `root` باشد (جلوگیری از path traversal).
/// - قبل از هر تغییری یک بکاپ `<file>.jev-bak` گرفته می‌شود.
/// - `patch.unified_diff` باید یک unified diff معتبر باشد؛ در غیر این صورت
///   هیچ چیزی نوشته نمی‌شود و خطا برمی‌گردد (برخلاف نسخه‌ی قبلی که متن diff
///   را مستقیم داخل فایل سورس می‌نوشت و آن را خراب می‌کرد).
pub fn apply_patch(root: &Path, patch: &PatchSuggestion) -> Result<PathBuf> {
    let target = resolve_target(root, &patch.file_path)?;

    if patch.unified_diff.trim().is_empty() {
        bail!("پچ خالی است — چیزی برای اعمال وجود ندارد");
    }

    let original = if target.exists() {
        fs::read_to_string(&target)
            .with_context(|| format!("خواندن فایل هدف ناموفق بود: {}", target.display()))?
    } else {
        String::new()
    };

    let parsed =
        diffy::Patch::from_str(&patch.unified_diff).context("متن پچ یک unified diff معتبر نیست")?;

    if parsed.hunks().is_empty() {
        bail!("متن پچ یک unified diff معتبر نیست (هیچ hunkای پیدا نشد)");
    }

    let patched = diffy::apply(&original, &parsed)
        .context("اعمال پچ روی فایل هدف ناموفق بود (context/hunk ناسازگار است)")?;

    // backup قبل از اعمال — بازگشت‌پذیری برای کاربر حیاتی است
    let backup = backup_path_for(&target);
    if target.exists() {
        fs::copy(&target, &backup)
            .with_context(|| format!("ساخت بکاپ ناموفق بود: {}", backup.display()))?;
    }

    if let Some(parent) = target.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("ساخت دایرکتوری والد ناموفق بود: {}", parent.display()))?;
        }
    }
    fs::write(&target, patched)
        .with_context(|| format!("نوشتن فایل وصله‌شده ناموفق بود: {}", target.display()))?;

    Ok(backup)
}

/// مسیر بکاپ برای یک فایل هدف: `<file>.jev-bak`
/// مثال: `src/main.rs` -> `src/main.rs.jev-bak`
pub fn backup_path_for(target: &Path) -> PathBuf {
    let mut s = target.as_os_str().to_owned();
    s.push(".jev-bak");
    PathBuf::from(s)
}

/// آخرین بکاپ را برمی‌گرداند (`jev undo`).
pub fn restore_backup(target: &Path) -> Result<()> {
    let backup = backup_path_for(target);
    if !backup.exists() {
        bail!("بکاپی برای {} پیدا نشد", target.display());
    }
    fs::copy(&backup, target).with_context(|| "بازیابی بکاپ ناموفق بود")?;
    Ok(())
}

/// مطمئن می‌شود مسیر پچ از `root` بیرون نمی‌زند.
fn resolve_target(root: &Path, file_path: &str) -> Result<PathBuf> {
    let rel = Path::new(file_path);
    if rel.is_absolute() {
        bail!("مسیر پچ باید نسبی باشد، نه مطلق: {}", file_path);
    }
    for comp in rel.components() {
        match comp {
            Component::ParentDir => bail!("مسیر پچ نباید حاوی `..` باشد: {}", file_path),
            Component::Prefix(_) | Component::RootDir => {
                bail!("مسیر پچ نامعتبر است: {}", file_path)
            }
            _ => {}
        }
    }
    Ok(root.join(rel))
}

/// محل ذخیره‌ی آخرین پچ پیشنهادی: `<root>/.jev/last-patch.json`
pub fn patch_store_path(root: &Path) -> PathBuf {
    root.join(".jev").join("last-patch.json")
}

/// ذخیره‌ی پیشنهاد برای مصرف بین `run` و `show/apply` در ترمینال دیگر.
pub fn save_suggestion(root: &Path, patch: &PatchSuggestion) -> Result<PathBuf> {
    let path = patch_store_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(patch)?;
    fs::write(&path, json)?;
    // نسخه‌ی خوانا برای انسان هم کنارش
    let _ = fs::write(
        root.join(".jev").join("last-patch.diff"),
        &patch.unified_diff,
    );
    Ok(path)
}

pub fn load_suggestion(root: &Path) -> Result<PatchSuggestion> {
    let path = patch_store_path(root);
    let data = fs::read_to_string(&path).with_context(|| {
        format!(
            "پچی ذخیره نشده ({} پیدا نشد). اول `jev run` را اجرا کنید",
            path.display()
        )
    })?;
    Ok(serde_json::from_str(&data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("jev_{}_{}_{}", name, std::process::id(), nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn suggestion(file: &str, diff: &str) -> PatchSuggestion {
        PatchSuggestion {
            file_path: file.to_string(),
            unified_diff: diff.to_string(),
            explanation: "test".to_string(),
        }
    }

    #[test]
    fn applies_hunk_and_keeps_rest_of_file() {
        let root = unique_dir("apply");
        let target_rel = "src/main.rs";
        fs::create_dir_all(root.join("src")).unwrap();
        let original = "fn main() {\n    let x: i32 = \"oops\";\n}\n";
        fs::write(root.join(target_rel), original).unwrap();

        let diff = "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let x: i32 = \"oops\";\n+    let x: i32 = 42;\n }\n";
        let backup = apply_patch(&root, &suggestion(target_rel, diff)).unwrap();

        let patched = fs::read_to_string(root.join(target_rel)).unwrap();
        assert!(patched.contains("let x: i32 = 42;"));
        assert!(patched.contains("fn main()"));
        // بکاپ باید محتوای اصلی را داشته باشد
        assert_eq!(fs::read_to_string(backup).unwrap(), original);
        // بازیابی باید برگرداند
        restore_backup(&root.join(target_rel)).unwrap();
        assert_eq!(fs::read_to_string(root.join(target_rel)).unwrap(), original);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_invalid_diff_without_touching_file() {
        let root = unique_dir("invalid");
        let target_rel = "a.txt";
        fs::write(root.join(target_rel), "hello\n").unwrap();
        let err = apply_patch(&root, &suggestion(target_rel, "این diff نیست")).unwrap_err();
        assert!(format!("{:#}", err).contains("unified diff"));
        // فایل نباید خراب شده باشد (باگ قبلی: متن diff داخل فایل نوشته می‌شد)
        assert_eq!(
            fs::read_to_string(root.join(target_rel)).unwrap(),
            "hello\n"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_path_traversal() {
        let root = unique_dir("traversal");
        let err = apply_patch(&root, &suggestion("../evil.rs", "x")).unwrap_err();
        assert!(format!("{:#}", err).contains(".."));
        let _ = fs::remove_dir_all(&root);
    }
}
