# jev-terminal-doctor 🩺

> Live terminal assistant — detects build/test errors as they happen and prepares a fix patch. Part of the `jev` ecosystem.
>
> دستیار زنده ترمینال — خطاهای build/test را در لحظه تشخیص می‌دهد و پچ پیشنهادی آماده می‌کند.

## How it works

`jev run` wraps your shell in a PTY, streams stdout/stderr, and matches known error patterns (rustc, cargo test, node, python, go). On a hit it:

1. collects context (project kind, `git diff`, file slice around the error line),
2. asks AI (Anthropic, or offline `mock` when no key is set),
3. saves the suggestion to `.jev/last-patch.json`,
4. notifies you in-terminal.

In another terminal (or after exit):

```bash
jev show    # view the proposal
jev apply   # hunk-by-hunk apply with .jev-bak backup
jev undo    # restore from backup
```

No key? No problem — mock mode lets you demo the full pipeline offline.

## Quickstart

```bash
cargo build --release
# optional (without it, jev runs in mock/demo mode):
export ANTHROPIC_API_KEY=...
export JEV_MODEL=claude-sonnet-4-6   # optional override

# terminal 1:
cargo run -- run

# cause an error in the wrapped shell, e.g.:
#   cargo test
#   python broken.py

# terminal 2 (same repo):
cargo run -- show
cargo run -- apply
cargo run -- undo
```

Patches are applied **hunk-by-hunk** with `diffy` (never raw overwrite), paths are confined to the repo root (`..`/absolute rejected), and every apply keeps a `<file>.jev-bak`.

## Architecture

```
src/
├── main.rs        # CLI: run / show / apply / undo / install-hook
├── pty/           # PTY wrap, passthrough, resize, analyzer loop
├── errors/        # heuristic detectors (rust/node/python/go) + ANSI stripping
├── context/       # project-kind + truncated git diff + file slice
├── ai/            # PatchProvider trait, AnthropicClient, MockProvider
└── tui/           # diff view + hunk apply + patch store (.jev/)
```

## Status / Roadmap

- [x] PTY wrap with stdin/resize passthrough
- [x] hunk-by-hunk patch apply (`diffy`) + backup + `undo`
- [x] end-to-end pipeline (detect → context → mock/anthropic → save → notify)
- [x] `show` / `apply` / `undo` commands
- [ ] live TUI overlay (currently conflicts with transparent passthrough — by design deferred)
- [ ] shell-hook mode (`PROMPT_COMMAND` / `precmd`) for non-PTY use
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
