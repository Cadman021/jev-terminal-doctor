use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::errors::detector;
use crate::pipeline::{analyze_and_suggest, provider_name};
use crate::tui::actions::patch_store_path;

/// RAII guard that disables terminal raw mode on every exit path
/// (success, error, or panic). Without this, a Jev crash would leave
/// the user's shell in raw mode and the terminal would look broken —
/// a classic, annoying bug in all PTY-wrapping tools.
struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Run the user's shell inside a PTY so Jev sits between the user and
/// the real shell, watching output without disturbing normal interaction.
///
/// Three parallel paths:
/// 1. User stdin -> written into the pty master (normal typing works)
/// 2. Pty master output -> printed to the real stdout + buffered for analysis
/// 3. User terminal resizes -> forwarded to the pty (tools like
///    vim/htop render correctly inside the wrapped shell)
pub async fn run_wrapped_shell(shell_override: Option<String>) -> Result<()> {
    let (shell, extra_args): (String, Vec<String>) = match shell_override {
        Some(s) => (s, vec![]),
        None => default_shell(),
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    println!(
        "[jev] Wrapping '{}' in {} — type `exit` to leave (Ctrl+C goes to the shell).",
        shell,
        cwd.display()
    );
    // Mark the window/tab title so the nested shell is distinguishable
    // from the outer one (their prompts look identical otherwise).
    print!("\x1b]0;[jev] {}\x07", cwd.display());
    let _ = std::io::Write::flush(&mut std::io::stdout());

    let (initial_cols, initial_rows) = crossterm::terminal::size().unwrap_or((80, 24));

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: initial_rows,
        cols: initial_cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new(&shell);
    for arg in &extra_args {
        cmd.arg(arg);
    }
    // Start the wrapped shell in the same directory, not the home dir.
    cmd.cwd(&cwd);
    // Lets Jev detect which shell it is inside of.
    cmd.env("JEV_ACTIVE", "1");
    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    // Raw mode must be enabled so keys like Ctrl+C and arrow keys reach
    // the underlying shell instead of being handled by the local terminal.
    let _raw_guard = RawModeGuard::enable()?;

    let running = Arc::new(AtomicBool::new(true));

    // ---------- 1. Forward user stdin to the pty ----------
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let writer_for_stdin = Arc::clone(&writer);
    let running_for_stdin = Arc::clone(&running);
    let stdin_thread = thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        while running_for_stdin.load(Ordering::SeqCst) {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut w = writer_for_stdin.lock().unwrap();
                    if w.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = w.flush();
                }
                Err(_) => break,
            }
        }
    });

    // ---------- 2. Read pty output, print it, and analyze it for errors ----------
    let mut reader = pair.master.try_clone_reader()?;
    let (tx, rx) = channel::<String>();
    let running_for_reader = Arc::clone(&running);
    let output_thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let mut stdout = std::io::stdout();
                    let _ = stdout.write_all(&buf[..n]);
                    let _ = stdout.flush();
                    let _ = tx.send(chunk);
                }
                Err(_) => break,
            }
        }
        running_for_reader.store(false, Ordering::SeqCst);
    });

    // ---------- 3. Watch terminal resizes and forward them to the pty ----------
    let master_for_resize = pair.master;
    let running_for_resize = Arc::clone(&running);
    let resize_thread = thread::spawn(move || {
        let mut last_size = (initial_cols, initial_rows);
        while running_for_resize.load(Ordering::SeqCst) {
            if let Ok(current) = crossterm::terminal::size() {
                if current != last_size {
                    let _ = master_for_resize.resize(PtySize {
                        rows: current.1,
                        cols: current.0,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                    last_size = current;
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    });

    // ---------- Analyzer: buffer output, detect errors, run the full pipeline ----------
    // detect -> collect context -> suggest patch (anthropic or mock) -> save to .jev/ -> notify.
    // There is intentionally no live overlay (it fights transparent PTY
    // passthrough); instead the patch is stored in `.jev/last-patch.json`
    // and the user runs `jev show/apply` in another terminal.
    let analyzer_thread = thread::spawn(move || {
        let mut buffer = String::new();
        let mut last_trigger = std::time::Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(std::time::Instant::now);
        let mut last_snippet = String::new();

        // Separate runtime for calling the async provider from a sync thread.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();

        const COOLDOWN: Duration = Duration::from_secs(15);
        const MAX_BUF: usize = 32 * 1024;

        while let Ok(chunk) = rx.recv() {
            buffer.push_str(&chunk);
            // Sliding window: keep only the last 32KB.
            if buffer.len() > MAX_BUF {
                let mut cut = buffer.len() - MAX_BUF;
                while cut < buffer.len() && !buffer.is_char_boundary(cut) {
                    cut += 1;
                }
                buffer.drain(..cut);
            }

            let Some(finding) = detector::scan(&buffer) else {
                continue;
            };

            // Dedup: do not report the same error over and over.
            if finding.raw_snippet == last_snippet && last_trigger.elapsed() < COOLDOWN * 4 {
                buffer.clear();
                continue;
            }
            if last_trigger.elapsed() < COOLDOWN {
                continue;
            }
            last_trigger = std::time::Instant::now();
            last_snippet = finding.raw_snippet.clone();

            let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let patch_result = match &rt {
                Ok(rt) => rt.block_on(analyze_and_suggest(&root, &finding)),
                Err(e) => Err(anyhow::anyhow!("failed to build runtime: {:#}", e)),
            };

            match patch_result {
                Ok(patch) => {
                    let path = patch_store_path(&root);
                    eprint!(
                        "\r\n[jev] [{:?}] error in {} -> patch ready ({})\r\n   {} \r\n   Run: `jev show` to view, `jev apply` to apply, `jev undo` to revert\r\n   File: {}\r\n",
                        finding.toolchain,
                        finding.file_hint.as_deref().unwrap_or("?"),
                        provider_name(),
                        patch.explanation,
                        path.display(),
                    )
                }
                Err(e) => {
                    eprint!("\r\n[jev] patch pipeline failed: {:#}\r\n", e);
                }
            }
            buffer.clear();
        }
    });

    // Wait for the child (shell) to exit — the natural end of the session.
    let status = tokio::task::spawn_blocking(move || child.wait()).await??;

    running.store(false, Ordering::SeqCst);
    let _ = output_thread.join();
    let _ = resize_thread.join();
    let _ = analyzer_thread.join();
    // stdin_thread is usually blocked on stdin.read(); detach it instead of
    // waiting (it dies with the main process on exit).
    drop(stdin_thread);

    // RawModeGuard drops here, terminal is back to normal — safe to print.
    println!(
        "[jev] Shell exited ({}). If a patch was prepared, run `jev show` / `jev apply` in the repo dir.",
        status
    );

    Ok(())
}

/// Pick a sensible default shell:
/// - explicit `--shell` wins (handled by the caller),
/// - on Unix respect `$SHELL`, fall back to `/bin/bash`,
/// - on Windows prefer PowerShell when the parent session is PowerShell
///   (`PSModulePath` set), otherwise fall back to `COMSPEC` (usually cmd).
///   Spawning cmd for a PowerShell user is confusing (different prompt,
///   different startup dir), so PowerShell comes first.
fn default_shell() -> (String, Vec<String>) {
    if let Ok(sh) = std::env::var("SHELL") {
        if !sh.trim().is_empty() {
            return (sh, vec![]);
        }
    }
    if cfg!(windows) {
        if std::env::var("PSModulePath").is_ok() {
            return ("powershell.exe".to_string(), vec!["-NoLogo".to_string()]);
        }
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        return (comspec, vec![]);
    }
    ("/bin/bash".to_string(), vec![])
}
