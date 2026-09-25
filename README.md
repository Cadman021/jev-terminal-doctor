# jev-terminal-doctor 🩺

> Live terminal assistant — detects build/test errors as they happen and prepares a fix patch. Part of the `jev` ecosystem.
>
> دستیار زنده ترمینال — خطاهای build/test را در لحظه تشخیص می‌دهد و پچ پیشنهادی آماده می‌کند.

## How it works

Two flows, same pipeline (detect → context → AI/mock → save to `.jev/last-patch.json`):

**A. Without nesting (recommended): run one command under `jev`.**
No wrapped shell, no second terminal confusion:

```bash
jev exec -- cargo test
jev exec -- python broken.py
```

Output streams live, the exit code is propagated, and on an error
pattern you get a patch hint right away. Then as usual:

```bash
jev show    # view the proposal
jev apply   # hunk-by-hunk apply with .jev-bak backup
jev undo    # restore from backup
```

Piped output works too:

```bash
cargo test 2>&1 | jev check --exit-code 1
```

**B. Interactive PTY wrap:** `jev run` wraps your shell and watches all
output. Heavier (nested shell); use it only for long sessions.

Shell helper (defines `jev-run`, a wrapper that captures failures for
`jev check` — PowerShell, bash, zsh):

```bash
jev install-hook --shell powershell   # print snippet
jev install-hook --shell bash --write # append to ~/.bashrc
```

No key? No problem — mock mode lets you demo the full pipeline offline.

## Quickstart

```bash
# Install (rebuild after each `git pull` so `jev` picks up fixes):
cargo install --path .
# ...or run without installing:
# cargo run -- <command>

# optional (without it, jev runs in mock/demo mode):
# PowerShell: $env:ANTHROPIC_API_KEY="..."
export ANTHROPIC_API_KEY=...
export JEV_MODEL=claude-sonnet-4-6   # optional override

# terminal 1 — starts a wrapped shell in the CURRENT directory:
jev run
# (override the shell if needed: jev run --shell powershell.exe)

# cause an error in the wrapped shell, e.g.:
#   cargo test
#   python broken.py

# terminal 2 (same repo dir):
jev show
jev apply
jev undo
```

> Windows: `jev run` spawns PowerShell when the parent session is
> PowerShell, otherwise `%COMSPEC%`. The wrapped shell starts in the
> same directory and `exit` ends the session. If you see a stale
> `jev` (old messages/behavior), reinstall with `cargo install --path .`.

Patches are applied **hunk-by-hunk** with `diffy` (never raw overwrite), paths are confined to the repo root (`..`/absolute rejected), and every apply keeps a `<file>.jev-bak`.

## Architecture

```
src/
├── main.rs        # CLI: run / exec / check / show / apply / undo / install-hook
├── pty/           # PTY wrap, passthrough, resize, analyzer loop
├── hook.rs        # non-PTY flows: exec, check, shell snippets
├── pipeline.rs    # shared detect→context→suggest→save pipeline
├── errors/        # heuristic detectors (rust/node/python/go/java) + ANSI stripping
├── context/       # project-kind + staged/unstaged/untracked diff + file slice
├── ai/            # PatchProvider trait, AnthropicClient, MockProvider
└── tui/           # diff view + hunk apply + patch store (.jev/)
```

## Status / Roadmap

- [x] PTY wrap with stdin/resize passthrough
- [x] hunk-by-hunk patch apply (`diffy`) + backup + `undo`
- [x] end-to-end pipeline (detect → context → mock/anthropic → save → notify)
- [x] `show` / `apply` / `undo` commands
- [x] non-PTY flows: `exec`, `check` (pipe/file), `install-hook` (`jev-run` for powershell/bash/zsh)
- [ ] live TUI overlay (currently conflicts with transparent passthrough — by design deferred)
- [ ] more providers (OpenAI, Ollama/local)
- [ ] asciinema demo + `cargo install` release binaries

## Safety

- unified-diff is parsed and validated before touching disk; invalid/empty patches are rejected without writes,
- path traversal blocked, writes stay inside repo root,
- `.jev/` and `*.jev-bak` are gitignored — review with `jev show` before `jev apply`.

## ایده (فارسی)

`jev run` شل شما را wrap می‌کند و خروجی را استریم می‌خواند. با دیدن ارور کامپایل/تست، کانتکست جمع می‌کند (نوع پروژه، دیف گیت، برش فایل)، از AI (یا mock آفلاین) پچ می‌گیرد و در `.jev/last-patch.json` ذخیره می‌کند. بعد با `jev show/apply/undo` در ترمینال دیگر بازبینی و اعمال می‌کنید.

## License

GPL-3.0-only — see [LICENSE](LICENSE).
