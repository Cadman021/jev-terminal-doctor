use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ai::PatchSuggestion;

// Deferred: kept for the live overlay; allow keeps CI green.
#[allow(dead_code)]
pub enum UserAction {
    Apply,
    Edit,
    Dismiss,
}

/// Map a pressed key to a UserAction.
/// Ctrl+F = Apply (fix), Ctrl+E = Edit, Esc = Dismiss.
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

/// Apply a patch to the target file, hunk by hunk.
///
/// - `patch.file_path` must be relative and inside `root` (path traversal protection).
/// - A `<file>.jev-bak` backup is taken before any change.
/// - `patch.unified_diff` must be a valid unified diff; otherwise nothing
///   is written and an error is returned (unlike the old version, which
///   wrote raw diff text into the source file and corrupted it).
pub fn apply_patch(root: &Path, patch: &PatchSuggestion) -> Result<PathBuf> {
    let target = resolve_target(root, &patch.file_path)?;

    if patch.unified_diff.trim().is_empty() {
        bail!("Patch is empty — nothing to apply");
    }

    let original = if target.exists() {
        fs::read_to_string(&target)
            .with_context(|| format!("Failed to read target file: {}", target.display()))?
    } else {
        String::new()
    };

    let parsed =
        diffy::Patch::from_str(&patch.unified_diff).context("Patch is not a valid unified diff")?;

    if parsed.hunks().is_empty() {
        bail!("Patch is not a valid unified diff (no hunks found)");
    }

    let patched = diffy::apply(&original, &parsed)
        .context("Failed to apply patch to target file (incompatible context/hunk)")?;

    // Backup before applying — reversibility is critical for users.
    let backup = backup_path_for(&target);
    if target.exists() {
        fs::copy(&target, &backup)
            .with_context(|| format!("Failed to create backup: {}", backup.display()))?;
    }

    if let Some(parent) = target.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent dir: {}", parent.display()))?;
        }
    }
    fs::write(&target, patched)
        .with_context(|| format!("Failed to write patched file: {}", target.display()))?;

    Ok(backup)
}

/// Backup path for a target file: `<file>.jev-bak`.
/// Example: `src/main.rs` -> `src/main.rs.jev-bak`
pub fn backup_path_for(target: &Path) -> PathBuf {
    let mut s = target.as_os_str().to_owned();
    s.push(".jev-bak");
    PathBuf::from(s)
}

/// Restore the last backup (`jev undo`).
pub fn restore_backup(target: &Path) -> Result<()> {
    let backup = backup_path_for(target);
    if !backup.exists() {
        bail!("No backup found for {}", target.display());
    }
    fs::copy(&backup, target).with_context(|| "Failed to restore backup")?;
    Ok(())
}

/// Make sure the patch path does not escape `root`.
fn resolve_target(root: &Path, file_path: &str) -> Result<PathBuf> {
    let rel = Path::new(file_path);
    if rel.is_absolute() {
        bail!("Patch path must be relative, not absolute: {}", file_path);
    }
    for comp in rel.components() {
        match comp {
            Component::ParentDir => bail!("Patch path must not contain `..`: {}", file_path),
            Component::Prefix(_) | Component::RootDir => {
                bail!("Invalid patch path: {}", file_path)
            }
            _ => {}
        }
    }
    Ok(root.join(rel))
}

/// Storage location of the last patch suggestion: `<root>/.jev/last-patch.json`
pub fn patch_store_path(root: &Path) -> PathBuf {
    root.join(".jev").join("last-patch.json")
}

/// Store a suggestion for consumption between `run` and `show/apply` in another terminal.
pub fn save_suggestion(root: &Path, patch: &PatchSuggestion) -> Result<PathBuf> {
    let path = patch_store_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(patch)?;
    fs::write(&path, json)?;
    // Human-readable copy next to it.
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
            "No stored patch ({} not found). Run `jev run` first",
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
        // Backup must hold the original content.
        assert_eq!(fs::read_to_string(backup).unwrap(), original);
        // Restore must bring it back.
        restore_backup(&root.join(target_rel)).unwrap();
        assert_eq!(fs::read_to_string(root.join(target_rel)).unwrap(), original);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_invalid_diff_without_touching_file() {
        let root = unique_dir("invalid");
        let target_rel = "a.txt";
        fs::write(root.join(target_rel), "hello\n").unwrap();
        let err = apply_patch(&root, &suggestion(target_rel, "not a diff")).unwrap_err();
        assert!(format!("{:#}", err).contains("unified diff"));
        // The file must not be corrupted (old bug: diff text was written into the file).
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
