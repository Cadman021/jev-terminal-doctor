mod ai;
mod context;
mod errors;
mod pty;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// jev-terminal-doctor — دستیار زنده و خودکار ترمینال
#[derive(Parser)]
#[command(
    name = "jev",
    version,
    about = "دستیار خودکار ترمینال برای تشخیص و پچ خطاها"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// شلی که باید wrap بشه (پیش‌فرض: $SHELL کاربر)
    #[arg(short, long)]
    shell: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// اجرای دمون در حالت تعاملی (پیش‌فرض)
    Run,
    /// نمایش آخرین پچ پیشنهادی ذخیره‌شده در .jev/
    Show,
    /// اعمال آخرین پچ پیشنهادی (با بکاپ .jev-bak)
    Apply,
    /// برگرداندن آخرین اعمال (بازیابی از .jev-bak)
    Undo {
        /// مسیر فایل برای undo (پیش‌فرض: فایل داخل آخرین پچ)
        file: Option<String>,
    },
    /// فقط نصب shell-hook بدون اجرای دمون (برای حالت دوم: precmd/PROMPT_COMMAND)
    InstallHook,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => {
            println!("🩺 jev-terminal-doctor در حال اجرا... (Ctrl+C برای خروج)");
            pty::interceptor::run_wrapped_shell(cli.shell).await?;
        }
        Commands::Show => {
            let patch = tui::actions::load_suggestion(&root)?;
            println!(
                "فایل: {}\nتوضیح: {}\n\n{}",
                patch.file_path, patch.explanation, patch.unified_diff
            );
        }
        Commands::Apply => {
            let patch = tui::actions::load_suggestion(&root)?;
            let backup = tui::actions::apply_patch(&root, &patch)?;
            println!(
                "✅ پچ روی {} اعمال شد (بکاپ: {})",
                patch.file_path,
                backup.display()
            );
        }
        Commands::Undo { file } => {
            let target = match file {
                Some(f) => root.join(f),
                None => {
                    let patch = tui::actions::load_suggestion(&root)?;
                    root.join(&patch.file_path)
                }
            };
            tui::actions::restore_backup(&target)?;
            println!("↩️ {} به نسخه‌ی قبل از پچ برگشت", target.display());
        }
        Commands::InstallHook => {
            println!("نصب shell hook هنوز پیاده‌سازی نشده — TODO");
        }
    }

    Ok(())
}
