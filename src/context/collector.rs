use anyhow::Result;
use git2::{Repository, StatusOptions};
use std::path::Path;

use super::{project, ProjectContext};
use crate::errors::ErrorFinding;

/// Build the context needed for a good AI prompt from an ErrorFinding
/// (coming from errors::detector): project kind, current git diff
/// (uncommitted changes that likely caused the error), and the error file.
///
/// The diff covers staged + unstaged changes plus a preview of untracked
/// files (new files are a common cause of "works on my machine" errors).
///
/// Caps (prevent token blowups): diff max 8KB, each file max 60 lines
/// around `line_hint` and max 6KB.
pub fn collect(root: &Path, finding: &ErrorFinding) -> Result<ProjectContext> {
    let mut ctx = ProjectContext {
        project_kind: project::detect_project_kind(root).map(|s| s.to_string()),
        ..Default::default()
    };

    // Uncommitted-changes diff — usually what introduced the error.
    if let Ok(repo) = Repository::open(root) {
        if let Ok(diff_text) = diff_uncommitted(&repo, root) {
            if !diff_text.trim().is_empty() {
                ctx.relevant_diff = Some(truncate_chars(&diff_text, 8000));
            }
        }
    }

    // Content of the file the compiler/test runner pointed at (sliced around the line).
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

/// Return `context_lines` lines before and after `line_1based`.
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

/// Combined staged (HEAD->index) + unstaged (index->workdir) diff,
/// plus a capped preview of untracked files.
fn diff_uncommitted(repo: &Repository, root: &Path) -> Result<String> {
    let mut out = String::new();

    // Staged: HEAD tree vs index. Missing HEAD (unborn branch) means
    // everything staged is new — diff against an empty tree.
    if let Ok(staged) = staged_diff(repo) {
        append_diff(&staged, &mut out)?;
    }
    // Unstaged: index vs workdir.
    let unstaged = repo.diff_index_to_workdir(None, None)?;
    append_diff(&unstaged, &mut out)?;

    // Untracked files (capped): new files never appear in diffs.
    append_untracked(repo, root, &mut out);

    Ok(out)
}

fn staged_diff(repo: &Repository) -> Result<git2::Diff<'_>> {
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    Ok(repo.diff_tree_to_index(head_tree.as_ref(), None, None)?)
}

fn append_diff(diff: &git2::Diff, out: &mut String) -> Result<()> {
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        if let Ok(content) = std::str::from_utf8(line.content()) {
            out.push_str(content);
        }
        true
    })?;
    Ok(())
}

/// Append up to 5 untracked files, 2KB each, so brand-new files that
/// break the build are visible to the AI.
fn append_untracked(repo: &Repository, root: &Path, out: &mut String) {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
        return;
    };
    let mut shown = 0;
    for entry in statuses.iter() {
        use git2::Status;
        if !entry.status().contains(Status::WT_NEW) {
            continue;
        }
        let Some(path_str) = entry.path() else {
            continue;
        };
        if shown >= 5 {
            out.push_str("... (more untracked files omitted)\n");
            break;
        }
        let path = root.join(path_str);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                out.push_str(&format!(
                    "\n--- new file: {}\n{}\n",
                    path_str,
                    truncate_chars(&content, 2000)
                ));
                shown += 1;
            }
            Err(_) => continue, // binary or unreadable — skip
        }
    }
}
