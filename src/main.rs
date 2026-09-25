mod ai;
mod context;
mod errors;
mod hook;
mod pipeline;
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

    /// Shell to wrap for `run` (default: auto-detected)
    #[arg(short, long)]
    shell: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the daemon in interactive PTY mode (default)
    Run,
    /// Run one command, stream its output, and suggest a patch on failure.
    /// No shell nesting: `jev exec -- cargo test`
    Exec {
        /// Command and args to run (everything after `--`).
        #[arg(last = true, required = true)]
        cmd: Vec<String>,
    },
    /// Analyze already-captured output (from a pipe or file) and suggest a patch.
    /// Example: `cargo test 2>&1 | jev check --exit-code 1`
    Check {
        /// Exit code of the command that produced the output.
        #[arg(long)]
        exit_code: Option<i32>,
        /// File with captured output (default: stdin).
        #[arg(long)]
        output_file: Option<std::path::PathBuf>,
        /// Repo root for context collection (default: current dir).
        #[arg(long)]
        cwd: Option<std::path::PathBuf>,
    },
    /// Show the last stored patch suggestion from .jev/
    Show,
    /// Apply the last patch suggestion (with .jev-bak backup)
    Apply,
    /// Revert the last apply (restore from .jev-bak)
    Undo {
        /// File path for undo (default: file from the last patch)
        file: Option<String>,
    },
    /// Print (or install with --write) a shell hook defining `jev-run`,
    /// a wrapper that captures failures for `jev check`. No PTY nesting.
    InstallHook {
        /// Target shell: powershell, bash, or zsh (default: auto-detected).
        #[arg(long)]
        shell: Option<String>,
        /// Append the snippet to the shell profile file.
        #[arg(long)]
        write: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => {
            pty::interceptor::run_wrapped_shell(cli.shell).await?;
        }
        Commands::Exec { cmd } => {
            let code = hook::run_exec(&root, &cmd).await?;
            std::process::exit(code);
        }
        Commands::Check {
            exit_code,
            output_file,
            cwd,
        } => {
            let check_root = cwd.unwrap_or(root);
            let output = hook::read_check_input(output_file)?;
            hook::run_check(&check_root, exit_code, &output).await?;
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
        Commands::InstallHook { shell, write } => {
            hook::install_hook(shell, write)?;
        }
    }

    Ok(())
}
