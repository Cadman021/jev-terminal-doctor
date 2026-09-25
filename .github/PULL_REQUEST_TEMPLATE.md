# Pull Request

## What does this change?
<!-- One paragraph: detector fix, new provider, TUI change... -->

## Why?
<!-- Link issue: Closes #... -->

## How was it tested?
- [ ] `cargo test`
- [ ] `cargo clippy -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] Manual: `jev run` -> trigger error -> `jev show` -> `jev apply` -> `jev undo`

## Safety checklist (required for patch/apply changes)
- [ ] Invalid/empty diffs are rejected without touching disk
- [ ] Backup (`.jev-bak`) created and restore verified
- [ ] No path traversal (absolute / `..` rejected, writes inside repo root)
- [ ] No secrets or API keys in logs/diffs

## Screenshots / terminal output
<!-- Paste `jev show` output or a short asciinema link -->
