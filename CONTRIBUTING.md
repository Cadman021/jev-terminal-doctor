# Contributing to jev-terminal-doctor

Thanks for helping! This project is GPL-3.0-only and part of the `jev` ecosystem.

## Quickstart

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

Run the daemon and exercise the pipeline (mock mode works without a key):

```bash
cargo run -- run
# in the wrapped shell, trigger an error, then in another terminal:
cargo run -- show
cargo run -- apply
cargo run -- undo
```

With a real key:

```bash
export ANTHROPIC_API_KEY=...
export JEV_MODEL=claude-sonnet-4-6  # optional
```

## Ground rules

- **English only** in code, comments, CLI output, and docs (Persian does not render reliably in all terminals).
- **Never write raw diff text into source files.** Patches go through `diffy::Patch::from_str` + `diffy::apply` in `src/tui/actions.rs`.
- **Safety first:** validate hunks, block path traversal (`..`/absolute), keep `<file>.jev-bak` backups, keep `.jev/` gitignored.
- **Bound AI costs:** truncate diffs (~8KB) and file slices (~60 lines / 6KB), debounce detection (~15s cooldown), dedup repeats.
- Keep the pipeline working offline: `MockProvider` must always produce an applicable diff.

## Project layout

- `src/pty/` — PTY wrap, passthrough, analyzer loop
- `src/errors/` — heuristic detectors per toolchain
- `src/context/` — project kind, git diff, file slices
- `src/ai/` — `PatchProvider` trait, Anthropic + mock
- `src/tui/` — diff view, apply/store, undo

## PR process

1. Open an issue first for anything beyond a typo (`bug_report.yml` / `feature_request.yml`).
2. Fill the PR template, including the safety checklist for apply/patch changes.
3. CI must be green: `cargo test`, `clippy -D warnings`, `fmt --check`.
4. One concern per PR; add/update tests for detectors and `apply_patch`.

## Reporting bugs

Include: version/commit, OS, repro steps, the triggering compiler output, and `.jev/last-patch.diff` if present. Never paste API keys.
