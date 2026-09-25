use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::ai::client::make_provider;
use crate::context::collector;
use crate::errors::detector;
use crate::tui::actions::save_suggestion;

/// RAII guard که raw mode ترمینال را در هر مسیر خروجی (موفق، خطا یا panic)
/// دوباره غیرفعال می‌کند. بدون این، اگر Jev کرش کند، شل کاربر در حالت raw
/// می‌ماند و ترمینال خراب به‌نظر می‌رسد — یک باگ کلاسیک و آزاردهنده در
/// همه‌ی ابزارهای PTY-wrapping.
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

/// اجرای شل کاربر داخل یک PTY، به‌طوری‌که Jev بین کاربر و شل واقعی
/// می‌نشیند و خروجی را رصد می‌کند بدون این‌که تعامل عادی کاربر مختل شود.
///
/// این نسخه سه مسیر موازی دارد:
/// ۱. stdin کاربر → نوشته می‌شود در pty master (تایپ عادی کار می‌کند)
/// ۲. خروجی pty master → چاپ می‌شود در stdout واقعی + بافر می‌شود برای تحلیل
/// ۳. تغییر اندازه‌ی ترمینال کاربر → به pty منتقل می‌شود (ابزارهایی مثل
///    vim/htop داخل شل wrapped درست رندر می‌شوند)
pub async fn run_wrapped_shell(shell_override: Option<String>) -> Result<()> {
    let shell = shell_override
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
            } else {
                "/bin/bash".to_string()
            }
        });

    let (initial_cols, initial_rows) = crossterm::terminal::size().unwrap_or((80, 24));

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: initial_rows,
        cols: initial_cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("JEV_ACTIVE", "1"); // برای این‌که خود Jev بتواند تشخیص بدهد داخل چه شلی است
    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    // raw mode باید فعال شود تا کلیدهایی مثل Ctrl+C, arrow keys و ... مستقیم
    // به شل زیرین برسند نه این‌که خود ترمینال محلی پردازششان کند
    let _raw_guard = RawModeGuard::enable()?;

    let running = Arc::new(AtomicBool::new(true));

    // ---------- ۱. فوروارد stdin کاربر به pty ----------
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

    // ---------- ۲. خواندن خروجی pty، چاپ به کاربر، و تحلیل برای خطا ----------
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

    // ---------- ۳. رصد تغییر اندازه‌ی ترمینال و انتقال آن به pty ----------
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

    // ---------- تحلیل‌گر: بافر می‌کند، خطا را تشخیص می‌دهد و پایپ‌لاین کامل را اجرا می‌کند ----------
    // detect -> collect context -> suggest patch (anthropic یا mock) -> save to .jev/ -> notify
    // عمداً overlay زنده نداریم (با passthrough شفاف PTY می‌جنگد)؛ در عوض پچ در
    // `.jev/last-patch.json` ذخیره می‌شود و کاربر در ترمینال دیگر `jev show/apply` می‌زند.
    let analyzer_thread = thread::spawn(move || {
        let mut buffer = String::new();
        let mut last_trigger = std::time::Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(std::time::Instant::now);
        let mut last_snippet = String::new();

        // ران‌تایم جدا برای صدا زدن provider ناهمگام از داخل thread همگام
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();

        const COOLDOWN: Duration = Duration::from_secs(15);
        const MAX_BUF: usize = 32 * 1024;

        while let Ok(chunk) = rx.recv() {
            buffer.push_str(&chunk);
            // پنجره‌ی لغزان: فقط 32KB آخر نگه داشته می‌شود
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

            // dedup: همان خطای قبلی را پشت سر هم گزارش نکن
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
            let ctx = match collector::collect(&root, &finding) {
                Ok(c) => c,
                Err(e) => {
                    eprint!("\r\n⚠️ Jev: جمع‌آوری کانتکست ناموفق بود: {:#}\r\n", e);
                    buffer.clear();
                    continue;
                }
            };

            let (provider, provider_name) = make_provider();
            let patch_result = match &rt {
                Ok(rt) => rt.block_on(provider.suggest_patch(&finding, &ctx)),
                Err(e) => Err(anyhow::anyhow!("ساخت ران‌تایم ناموفق بود: {:#}", e)),
            };

            match patch_result {
                Ok(patch) => {
                    match save_suggestion(&root, &patch) {
                        Ok(path) => eprint!(
                            "\r\n🔎 Jev [{:?}/{:?}] خطا در {} → پچ آماده شد ({})\r\n   {} \r\n   اجرا: `jev show` برای دیدن، `jev apply` برای اعمال، `jev undo` برای بازگشت\r\n   فایل: {}\r\n",
                            finding.toolchain,
                            ctx.project_kind.as_deref().unwrap_or("unknown"),
                            finding.file_hint.as_deref().unwrap_or("?"),
                            provider_name,
                            patch.explanation,
                            path.display(),
                        ),
                        Err(e) => eprint!("\r\n⚠️ Jev: ذخیره‌ی پچ ناموفق بود: {:#}\r\n", e),
                    }
                }
                Err(e) => {
                    eprint!("\r\n⚠️ Jev: تولید پچ ناموفق بود: {:#}\r\n", e);
                }
            }
            buffer.clear();
        }
    });

    // منتظر خروج فرزند (شل) می‌مانیم — این پایان طبیعی جلسه است
    let _ = tokio::task::spawn_blocking(move || child.wait()).await?;

    running.store(false, Ordering::SeqCst);
    let _ = output_thread.join();
    let _ = resize_thread.join();
    let _ = analyzer_thread.join();
    // stdin_thread معمولاً روی stdin.read() بلاک مانده؛ آن را detach می‌کنیم
    // تا منتظرش نمانیم (وقتی فرآیند اصلی خارج شود، خودش هم می‌میرد)
    drop(stdin_thread);

    Ok(())
}
