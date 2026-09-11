# ctos

**count tokens of skill** — a cloc-style, cross-platform token counter for Agent Skills and source code.

> ⚠️ *not related to Watch Dogs' ctOS.*

[![CI](https://github.com/leftvalue/ctos/actions/workflows/ci.yml/badge.svg)](https://github.com/leftvalue/ctos/actions/workflows/ci.yml)
![license](https://img.shields.io/badge/license-GPL--3.0-blue.svg)

`cloc` tells you how many **lines** of code you have. `ctos` tells you how many
**tokens** a directory costs — for any tokenizer you care about — and, when a
directory is an Agent Skill, how those tokens split across the layers that
actually get injected into a model's context.

Because so much code is now written by AI, "how many tokens is this?" is a
first-class question for source trees, not just skills. `ctos` answers both.

```
cloc:  lines of code   → files, blank, comment, code
ctos:  tokens of stuff → files, bytes, tokens   (+ skill L1/L2/L3 layers)
```

---

## Table of contents

- [Quick start](#quick-start)
- [Installation](#installation)
- [What it counts](#what-it-counts)
- [The three-layer skill model](#the-three-layer-skill-model)
- [Model matrix](#model-matrix)
- [Commands & options](#commands--options)
- [JSON output](#json-output)
- [CI integration](#ci-integration)
- [Configuration](#configuration)
- [Vendoring builtin tokenizers](#vendoring-builtin-tokenizers)
- [Calibration](#calibration)
- [Build from source](#build-from-source)
- [Design notes](#design-notes)
- [License](#license)

---

## Quick start

```bash
# count tokens of a whole source tree (all registered models)
ctos ./my-project

# just one model, cloc-style language table
ctos -m gpt-4o ./src

# per-file breakdown instead of language aggregation
ctos -m gpt-4o --by-file ./src

# a skill directory — code table PLUS L1/L2/L3 layer analysis
ctos -m gpt-4o ./skills/image-gen-retry

# machine-readable output for CI / baselines
ctos --format json ./skills > report.json

# budget gate (see CI section) — exits non-zero when a skill blows its budget
ctos check ./skills
```

Example (language aggregation + skill layers):

```
ctos v0.1.0 — count tokens of skill
root: /repo/skills

tokenizer: gpt-4o (tiktoken)
┌──────────┬───────┬─────────┬─────────┐
│ Language ┆ files ┆   bytes ┆  tokens │
╞══════════╪═══════╪═════════╪═════════╡
│ Markdown ┆     4 ┆  48.2 KB ┆  12,044 │
│ Python   ┆     2 ┆   3.1 KB ┆     812 │
│ Image    ┆     1 ┆ 120.0 KB ┆       0 │
│ SUM      ┆     7 ┆ 171.3 KB ┆  12,856 │
└──────────┴───────┴─────────┴─────────┘

Skill layers (L1 metadata / L2 body / L3 assets):
┌────────────────────────┬─────┬───────┬─────────┬───────────┬────────┐
│ skill                  ┆  L1 ┆    L2 ┆ L3(tok) ┆ L3(bytes) ┆ status │
╞════════════════════════╪═════╪═══════╪═════════╪═══════════╪════════╡
│ image-gen-retry        ┆  92 ┆ 3,412 ┆  12,044 ┆   48.2 KB ┆ OK     │
│ TOTAL (1 valid skills) ┆  92 ┆ 3,412 ┆  12,044 ┆   48.2 KB ┆        │
└────────────────────────┴─────┴───────┴─────────┴───────────┴────────┘
resident L1 total: 92 tok  →  peak injection: 3,504 tok
```

The last line is what capacity planning actually cares about: **resident cost**
(every skill's L1 is always in context) and **peak injection cost** (resident +
the single heaviest skill body that a trigger can pull in).

---

## Installation

### Option 1 — download a prebuilt binary (no toolchain needed)

Prebuilt static binaries are published on the **[Releases](../../releases)** page
for six targets (Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64):

```bash
# Linux/macOS example
curl -L -o ctos https://github.com/leftvalue/ctos/releases/download/v0.1.0/ctos-x86_64-unknown-linux-musl
chmod +x ctos
sudo mv ctos /usr/local/bin/         # or anywhere on your PATH
ctos --version
```

On Windows, download `ctos-x86_64-pc-windows-msvc.exe` and put it on your PATH.

> Releases appear only after a maintainer pushes a version tag — see
> [Cutting a release](#cutting-a-release).

### Option 2 — install from git with Cargo (no crates.io needed)

```bash
cargo install --git https://github.com/leftvalue/ctos --tag v0.1.0
# or the latest default branch:
cargo install --git https://github.com/leftvalue/ctos
```

This compiles locally and installs `ctos` into `~/.cargo/bin`. The vendored
builtin tokenizers are committed in the repo, so the build is fully offline.

### Option 3 — build from a clone

```bash
git clone https://github.com/leftvalue/ctos
cd ctos
cargo build --release        # binary at target/release/ctos
```

### Option 4 — crates.io

Not published yet (`publish = false`). If/when published:

```bash
cargo install ctos
```

### Option 5 — Docker (no toolchain, no install)

A tiny (~35 MB) `scratch`-based image with a fully static binary and the builtin
tokenizers baked in. Mount your project and pass paths under the mount point:

```bash
# build locally
docker build -t ctos .

# or pull the published image
docker pull ghcr.io/leftvalue/ctos:latest

# count the current directory
docker run --rm -v "$PWD:/work" ghcr.io/leftvalue/ctos /work

# a specific model + skill budget gate (exit code is preserved for CI)
docker run --rm -v "$PWD:/work" ghcr.io/leftvalue/ctos -m gpt-4o /work/skills
docker run --rm -v "$PWD:/work" ghcr.io/leftvalue/ctos check /work/skills

# meta commands
docker run --rm ghcr.io/leftvalue/ctos --version
docker run --rm ghcr.io/leftvalue/ctos models
```

> The container's working directory is `/work`; mount your project there and use
> `/work/...` paths. The image has no shell — `ctos` is the entrypoint.

### Cutting a release (maintainers)

Pushing code alone does **not** create binaries. The release workflow triggers on
a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions then vendors the tokenizers, cross-compiles all six targets,
attaches the binaries to a GitHub Release, and builds & pushes the Docker image to
`ghcr.io/leftvalue/ctos` (tagged with the version and `latest`). (`ci.yml` —
fmt/clippy/test — runs on every push/PR; only `release.yml` needs a tag.)

---

## What it counts

`ctos` walks the path (honoring `.gitignore`, like ripgrep) and, for every file:

| File kind | tokens | bytes | language |
|---|---|---|---|
| Plain text (code, markdown, config, …) | ✅ counted per model | ✅ | mapped by extension |
| Binary (images, fonts, archives, `.bin`, …) | — (skipped) | ✅ | best-effort label |

- **Text vs binary** is sniffed from a head sample (NUL byte or largely-invalid
  UTF-8 ⇒ binary).
- Files are read as UTF-8; invalid bytes are replaced with `U+FFFD` and counted
  — `ctos` never panics on bad encoding.
- Tokens are counted with **`add_special_tokens = false`**. Special/framing
  tokens are the client's job; `ctos` measures raw content plus (for L1) a
  fixed, configurable wrapping overhead.

---

## The three-layer skill model

An Agent Skill is a directory containing `SKILL.md` (YAML frontmatter + body),
optionally with `scripts/`, `references/`, `assets/`, etc. `ctos` splits it into
three layers that mirror *when* each part hits the model context:

```
skill/
├── SKILL.md
│   ├── frontmatter: name + description ─────────────┐
│   └── body (markdown)                              │
├── scripts/…                                        │
└── references/…                                     │
                                                     │
   L1  name + description (+ overhead)   ── resident, always injected    ≤ 100 tok
   L2  full frontmatter block + body     ── injected when the skill fires ≤ 5000 tok
   L3  every other text file             ── read on demand, never resident  (display only)
```

- **L1 = tokens(name + description) + `overhead_l1`** (default `24`, configurable).
  This is the always-on cost; the sum across all skills is your resident budget.
- **L2 = tokens(entire frontmatter block + body).**
  *Design choice:* the whole frontmatter block is counted into L2 (not just
  `name`/`description`). L1 is measured purely from `name`+`description`+overhead.
- **L3 = every text file in the skill dir except `SKILL.md`**, reported as both
  tokens and bytes. Binary assets contribute bytes only.
- A skill missing `name`/`description` or with unparseable YAML is flagged
  **`INVALID`** — it does not abort the run but does affect the exit code.

---

## Model matrix

`ctos` resolves a friendly model name to one of four tokenizer sources:

| Source                | Form                     | Example                         | Offline | Notes |
|-----------------------|--------------------------|---------------------------------|:-------:|-------|
| **builtin**           | `builtin:<key>`          | `builtin:qwen3`                 | ✅      | vendored `tokenizer.json` **or** `tiktoken.model` embedded into the binary |
| **file**              | `file:<path>`            | `file:/opt/models/tok.json`     | ✅      | any local `tokenizer.json` (intranet / long-tail models) |
| **tiktoken**          | `tiktoken:<encoding>`    | `tiktoken:o200k_base`           | ✅      | OpenAI encodings (`o200k_base`, `cl100k_base`, …) |
| **claude-approx**     | `claude-approx`          | —                               | ✅      | `chars / chars_per_token` estimate |

Default registry (`config/models.toml`): `qwen3`, `deepseek-v3`, `kimi-k2`,
`hunyuan` (all `builtin:`), `gpt-4o` (`tiktoken:o200k_base`), and `claude`
(`claude-approx`).

> **On Kimi-K2:** it does not publish a HuggingFace `tokenizer.json`; it ships a
> `tiktoken.model` BPE vocab plus a custom split pattern. `ctos` loads that model
> via tiktoken-rs using Kimi's **original** pattern (including its `&&`
> set-intersection subclasses, which `fancy-regex` accepts), so counts match the
> model exactly and stay fully offline.

> **On Claude:** there is no public Claude tokenizer. `claude` is an
> **approximation** (`chars / 4.0` by default) and its numbers are printed with
> a `~` prefix. In `check`, budgets are relaxed by ×1.1 for approximate models.
> `ctos` does **not** claim to precisely support Claude — calibrate it (below).

List everything the current build knows about:

```bash
ctos models
```

---

## Commands & options

```
ctos <PATH> [OPTIONS]          # default: count
ctos check <PATH> [OPTIONS]    # budget gate for CI
ctos models                    # list registry models & their sources
ctos calibrate --model <m> <PATH>   # per-layer counts for manual calibration
```

Common options:

| Option | Meaning | Default |
|---|---|---|
| `-m, --model <name>` | tokenizer to use; repeatable | all registry models |
| `--format table\|json` | output format | `table` |
| `-o, --output <path>` | write to a file | stdout |
| `--baseline <path>` | (`check`) baseline results JSON for growth diffing | none |
| `--budgets <path>` | custom `budgets.toml` | built-in defaults |
| `--models-config <path>` | custom `models.toml` | built-in defaults |
| `--no-ignore` | do not honor `.gitignore` | honored |
| `-v, --verbose` | per-file L3 detail / config fallbacks | off |
| `--by-file` | per-file code listing instead of language aggregation | off |
| `-q, --quiet` | minimal output | off |

---

## JSON output

`--format json` emits a stable, CI-friendly document. The `results` array
follows the pinned schema; a parallel `code` block carries the language/file
aggregation:

```json
{
  "tool": { "name": "ctos", "version": "0.1.0" },
  "models": ["gpt-4o"],
  "results": [
    {
      "skill": "image-gen-retry",
      "model": "gpt-4o",
      "l1": 92, "l2": 3412,
      "l3_tokens": 12044, "l3_bytes": 49357,
      "files": [{ "path": "scripts/render.py", "tokens": 12044 }],
      "status": "OK",
      "issues": []
    }
  ],
  "code": [
    {
      "model": "gpt-4o",
      "approx": false,
      "languages": [{ "language": "Markdown", "files": 4, "bytes": 48200, "tokens": 12044 }],
      "files": [{ "path": "…", "language": "Markdown", "bytes": 246, "is_binary": false, "tokens": 58 }]
    }
  ]
}
```

---

## CI integration

`ctos check` is the gate. Exit codes are a contract:

| Code | Meaning |
|:---:|---|
| `0` | all skills within budget |
| `1` | a skill is over budget / grew too much / is `INVALID` |
| `2` | runtime error (bad path, unparseable config, …) |

Drop-in GitHub Actions step (copy-paste):

```yaml
name: skill-budget
on: [pull_request]
jobs:
  ctos:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install ctos
        run: cargo install --git https://github.com/leftvalue/ctos --tag v0.1.0   # or download a release binary
      - name: Enforce skill budgets
        run: ctos check ./skills -m gpt-4o
```

With a baseline to catch silent growth:

```yaml
      - name: Fetch baseline (from main)
        run: git show origin/main:ctos-baseline.json > baseline.json || echo '{}' > baseline.json
      - name: Check growth
        run: ctos check ./skills -m gpt-4o --baseline baseline.json
```

A skill whose L2 grows more than `diff_ratio` (default 15%) fails **unless** its
`SKILL.md` contains a `ctos-ok-growth` marker (e.g. in an HTML comment),
acknowledging the intended growth.

---

## Configuration

Two TOML files, both overridable via `--models-config` / `--budgets`. When a
custom file is omitted, built-in defaults are used (announced under `-v`).

**`config/models.toml`** — the registry:

```toml
[models.qwen3]
source = "builtin:qwen3"
overhead_l1 = 24

[models.gpt-4o]
source = "tiktoken:o200k_base"

[models.claude]
source = "claude-approx"
chars_per_token = 4.0

[models.custom-internal]
source = "file:/opt/models/internal/tokenizer.json"
```

**`config/budgets.toml`** — the gate:

```toml
[global]
l1_max       = 100    # per-skill L1 upper bound
l2_max       = 5000   # per-skill L2 upper bound
l1_total_max = 2000   # sum of L1 across all skills (resident capacity)
diff_ratio   = 0.15   # allowed L2 growth vs baseline before it must be justified
```

---

## Vendoring builtin tokenizers

`builtin:` models embed their tokenizer file (`tokenizer.json` for HF models, or
`tiktoken.model` for Kimi-K2) **into the binary** at build time (via `build.rs` +
`include_bytes!`), so releases work fully offline.

The easiest path is the fetch script, which pins each file to an exact commit and
verifies it against `tokenizers/checksums.sha256`:

```bash
scripts/fetch-tokenizers.sh          # qwen3 + deepseek-v3 + kimi-k2 (default set)
scripts/fetch-tokenizers.sh hunyuan  # additionally fetch the opt-in Hunyuan model
cargo build --release
```

Or place a file manually under `tokenizers/<key>/` (`tokenizer.json` or
`tiktoken.model`) and rebuild.

If the file is **absent**, the build still succeeds and that `builtin:` model
reports a clear, actionable error at runtime — you can always fall back to
`tiktoken:`, `claude-approx`, or a `file:` source. See
[`tokenizers/README.md`](tokenizers/README.md) for details.

> **Licensing:** each vendored file is redistributed under its own model license,
> not ctos's GPL-3.0. Embedding it into a distributed binary is redistribution —
> see [`THIRD_PARTY_LICENSES/README.md`](THIRD_PARTY_LICENSES/README.md). Hunyuan
> is opt-in because its community license carries extra conditions.

> Pin the exact revision you download. Tokenizer changes shift counts, which the
> golden tests are designed to catch.

---

## Calibration

The L1 `overhead_l1` constant and the `claude-approx` `chars_per_token` divisor
approximate the real client's injection cost. To calibrate:

```bash
ctos calibrate --model claude ./skills/image-gen-retry
```

Compare `ctos`'s numbers with what your client reports (e.g. Claude Code's
`/skills` view), then adjust `overhead_l1` / `chars_per_token` in
`models.toml`. See [`calibration/README.md`](calibration/README.md). Automated
calibration is planned for v0.2.

---

## Build from source

Requires a recent stable Rust toolchain.

```bash
git clone https://github.com/leftvalue/ctos
cd ctos
cargo build --release       # binary at target/release/ctos
cargo test --all-features   # unit + golden + exit-code tests
```

Cross-platform static binaries are produced by the release workflow using
[`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) for six targets
(Linux x86_64/aarch64 musl, macOS x86_64/arm64, Windows x86_64).

Feature flags: `hf` (HuggingFace `tokenizers`, for `builtin:`/`file:`) and
`tiktoken` (OpenAI encodings) are both on by default; `claude-approx` needs
neither. Disable a feature to shrink the binary if you don't need it.

---

## Design notes

- **Measurement, not governance.** `ctos` does not rewrite skills, judge their
  quality, or simulate the full client injection context. It measures size and
  a fixed overhead constant. Clean boundaries keep the tool long-lived.
- **Deterministic & pinned.** Token counts are reproducible for a given
  `Cargo.lock`. Golden tests snapshot fixture counts so a tokenizer upgrade
  can't silently change your numbers.
- **Never aborts on bad input.** Unreadable files are skipped, bad encodings are
  lossily decoded, and `INVALID` skills are reported inline while the rest of
  the run completes.

---

## License

GPL-3.0. See [LICENSE](LICENSE).
