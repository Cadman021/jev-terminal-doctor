mod ai;
mod context;
mod errors;
mod pty;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// jev-terminal-doctor — live terminal assistant
#[derive(Parser)]
#[command(
    name = "jev",
    version,
    about = "Automatic terminal assistant for detecting and patching errors"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Shell to wrap (default: user's $SHELL)
    #[arg(short, long)]
    shell: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the daemon in interactive mode (default)
    Run,
    /// Show the last stored patch suggestion from .jev/
    Show,
    /// Apply the last patch suggestion (with .jev-bak backup)
    Apply,
    /// Revert the last apply (restore from .jev-bak)
    Undo {
        /// File path for undo (default: file from the last patch)
        file: Option<String>,
    },
    /// Install shell hook only, without running the daemon (precmd/PROMPT_COMMAND mode)
    InstallHook,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => {
            pty::interceptor::run_wrapped_shell(cli.shell).await?;
        }
        Commands::Show => {
            let patch = tui::actions::load_suggestion(&root)?;
            println!(
                "File: {}\nExplanation: {}\n\n{}",
                patch.file_path, patch.explanation, patch.unified_diff
            );
        }
        Commands::Apply => {
            let patch = tui::actions::load_suggestion(&root)?;
            let backup = tui::actions::apply_patch(&root, &patch)?;
            println!(
                "Patch applied to {} (backup: {})",
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
            println!("{} restored to the pre-patch version", target.display());
        }
        Commands::InstallHook => {
            println!("Shell hook installation is not implemented yet — TODO");
        }
    }

    Ok(())
}
