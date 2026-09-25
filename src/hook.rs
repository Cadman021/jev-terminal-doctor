use anyhow::{bail, Context, Result};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::errors::detector;
use crate::pipeline::{analyze_and_suggest, provider_name};

/// Marker written into shell profiles so `install-hook --write` is idempotent.
const HOOK_MARKER: &str = "# jev-terminal-doctor hook";

/// PowerShell wrapper: runs any command, streams output, then feeds the
/// captured output plus exit code to `jev check`.
pub fn powershell_snippet() -> &'static str {
    r#"# jev-terminal-doctor hook — paste into $PROFILE (or run `jev install-hook --shell powershell --write`)
function jev-run {
    $jevTmp = [System.IO.Path]::GetTempFileName()
    try {
        $cmd = $args[0]
        $rest = @()
        if ($args.Count -gt 1) { $rest = $args[1..($args.Count - 1)] }
        & $cmd @rest 2>&1 | Tee-Object -FilePath $jevTmp
        $code = 0
        if ($null -ne $LASTEXITCODE) { $code = $LASTEXITCODE } elseif (-not $?) { $code = 1 }
        jev check --exit-code $code --output-file $jevTmp
    } finally { Remove-Item $jevTmp -ErrorAction SilentlyContinue }
}
"#
}

/// Bash wrapper using PIPESTATUS so the real exit code survives the `tee` pipe.
pub fn bash_snippet() -> &'static str {
    r#"# jev-terminal-doctor hook — paste into ~/.bashrc (or run `jev install-hook --shell bash --write`)
jev-run() {
  local jev_tmp code
  jev_tmp="$(mktemp)" || return 1
  "$@" 2>&1 | tee "$jev_tmp"
  code="${PIPESTATUS[0]}"
  jev check --exit-code "$code" --output-file "$jev_tmp" --cwd "$PWD"
  rm -f "$jev_tmp"
  return "$code"
}
"#
}

/// Zsh wrapper (zsh uses lowercase `pipestatus`).
pub fn zsh_snippet() -> &'static str {
    r#"# jev-terminal-doctor hook — paste into ~/.zshrc (or run `jev install-hook --shell zsh --write`)
jev-run() {
  local jev_tmp code
  jev_tmp="$(mktemp)" || return 1
  "$@" 2>&1 | tee "$jev_tmp"
  code="${pipestatus[1]}"
  jev check --exit-code "$code" --output-file "$jev_tmp" --cwd "$PWD"
  rm -f "$jev_tmp"
  return "$code"
}
"#
}

fn snippet_for(kind: &str) -> Option<&'static str> {
    let k = kind.to_lowercase();
    if k.contains("powershell") || k.contains("pwsh") {
        Some(powershell_snippet())
    } else if k.contains("zsh") {
        Some(zsh_snippet())
    } else if k.contains("bash") || k == "sh" {
        Some(bash_snippet())
    } else {
        None
    }
}

fn detect_shell_kind() -> String {
    if cfg!(windows) {
        return "powershell".to_string();
    }
    std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string())
}

/// Print (and optionally install) the shell hook snippet.
pub fn install_hook(shell: Option<String>, write: bool) -> Result<()> {
    let kind = shell.unwrap_or_else(detect_shell_kind);
    let snippet = snippet_for(&kind).with_context(|| {
        format!(
            "Unsupported shell '{}'. Choose powershell, bash, or zsh.",
            kind
        )
    })?;

    if !write {
        println!(
            "{}\n# Reload your shell config afterwards (e.g. `. ~/.bashrc`).",
            snippet
        );
        return Ok(());
    }

    let profile = profile_path(&kind).with_context(|| {
        format!(
            "Cannot locate a profile file for '{}'. Re-run without --write and paste manually.",
            kind
        )
    })?;
    if let Some(parent) = profile.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = std::fs::read_to_string(&profile).unwrap_or_default();
    if existing.contains(HOOK_MARKER) {
        println!("[jev] hook already present in {}", profile.display());
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile)?;
    writeln!(f, "\n{}", snippet)?;
    println!(
        "[jev] hook appended to {}. Reload your shell.",
        profile.display()
    );
    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn profile_path(kind: &str) -> Option<PathBuf> {
    let k = kind.to_lowercase();
    let home = home_dir()?;
    if k.contains("powershell") || k.contains("pwsh") {
        if cfg!(windows) {
            let docs = home.join("Documents");
            let ps7 = docs
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1");
            if ps7.parent().map(|p| p.exists()).unwrap_or(false) {
                return Some(ps7);
            }
            return Some(
                docs.join("WindowsPowerShell")
                    .join("Microsoft.PowerShell_profile.ps1"),
            );
        }
        return Some(home.join(".config/powershell/Microsoft.PowerShell_profile.ps1"));
    }
    if k.contains("zsh") {
        return Some(home.join(".zshrc"));
    }
    Some(home.join(".bashrc"))
}

/// Read check input from a file, or stdin when piped.
/// Refuses to block on an interactive TTY with no input.
pub fn read_check_input(output_file: Option<PathBuf>) -> Result<String> {
    if let Some(path) = output_file {
        return std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()));
    }
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        bail!("No input: pipe command output into `jev check` or pass --output-file");
    }
    let mut buf = String::new();
    stdin
        .lock()
        .read_to_string(&mut buf)
        .context("Failed to read stdin")?;
    Ok(buf)
}

/// Analyze captured output and run the shared patch pipeline on a hit.
/// Pure reporting: always succeeds unless Jev itself errors.
pub async fn run_check(root: &Path, exit_code: Option<i32>, output: &str) -> Result<()> {
    let output = tail_chars(output, 256 * 1024);
    match detector::scan(&output) {
        Some(finding) => {
            let patch = analyze_and_suggest(root, &finding).await?;
            println!(
                "[jev] [{:?}] error in {} — patch ready ({})\n  {}\n  Next: `jev show` to view, `jev apply` to apply, `jev undo` to revert",
                finding.toolchain,
                finding.file_hint.as_deref().unwrap_or("unknown location"),
                provider_name(),
                patch.explanation,
            );
        }
        None => match exit_code {
            Some(code) if code != 0 => println!(
                "[jev] command failed (exit {}) but no known error pattern matched — nothing saved.",
                code
            ),
            _ => println!("[jev] no errors detected."),
        },
    }
    Ok(())
}

/// One-shot execution: run a command with piped output, stream it live,
/// then analyze and suggest a patch on error. Returns the child's exit code.
pub async fn run_exec(root: &Path, cmd: &[String]) -> Result<i32> {
    let (prog, args) = cmd
        .split_first()
        .context("Usage: jev exec -- <command> [args...]")?;

    let mut child = std::process::Command::new(prog)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn '{}'", prog))?;

    let stdout_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::<u8>::new()));

    let out_handle = spawn_streamer(child.stdout.take(), false, Arc::clone(&stdout_buf));
    let err_handle = spawn_streamer(child.stderr.take(), true, Arc::clone(&stderr_buf));

    let status = child.wait()?;
    let _ = out_handle.join();
    let _ = err_handle.join();

    let code = status
        .code()
        .unwrap_or_else(|| if status.success() { 0 } else { 1 });

    let combined = {
        let out = stdout_buf.lock().unwrap();
        let err = stderr_buf.lock().unwrap();
        let mut combined = String::from_utf8_lossy(&out).into_owned();
        combined.push('\n');
        combined.push_str(&String::from_utf8_lossy(&err));
        combined
    };

    run_check(root, Some(code), &combined).await?;
    Ok(code)
}

/// Forward a piped child stream to the matching terminal stream while
/// buffering a capped copy for analysis.
fn spawn_streamer(
    pipe: Option<impl Read + Send + 'static>,
    is_stderr: bool,
    buf: Arc<Mutex<Vec<u8>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if is_stderr {
                        let _ = std::io::stderr().write_all(&chunk[..n]);
                        let _ = std::io::stderr().flush();
                    } else {
                        let _ = std::io::stdout().write_all(&chunk[..n]);
                        let _ = std::io::stdout().flush();
                    }
                    let mut b = buf.lock().unwrap();
                    // Cap: keep the buffer bounded, drop the oldest half.
                    if b.len() > 512 * 1024 {
                        let drop_n = b.len() / 2;
                        b.drain(..drop_n);
                    }
                    b.extend_from_slice(&chunk[..n]);
                }
                Err(_) => break,
            }
        }
    })
}

fn tail_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_snippet_calls_check() {
        let s = powershell_snippet();
        assert!(s.contains("function jev-run"));
        assert!(s.contains("jev check"));
        assert!(s.contains(HOOK_MARKER));
    }

    #[test]
    fn bash_snippet_preserves_exit_code() {
        let s = bash_snippet();
        assert!(s.contains("PIPESTATUS"));
        assert!(s.contains("jev check"));
    }

    #[test]
    fn zsh_snippet_uses_pipestatus() {
        let s = zsh_snippet();
        assert!(s.contains("pipestatus"));
        assert!(s.contains("jev check"));
    }

    #[test]
    fn snippet_for_unknown_shell_is_none() {
        assert!(snippet_for("fish").is_none());
        assert!(snippet_for("powershell").is_some());
    }
}
