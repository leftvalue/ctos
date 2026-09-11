# Vendored tokenizers

`ctos` embeds each tokenizer file found here into the binary at build time (see
`build.rs`), so a released binary counts tokens **fully offline**. Two file kinds
are recognized per key:

* `tokenizer.json` — a HuggingFace tokenizer (loaded by the `tokenizers` crate)
* `tiktoken.model` — a tiktoken BPE vocab (loaded by `tiktoken-rs`)

## Layout

```
tokenizers/
├── qwen3/tokenizer.json          # HF
├── deepseek-v3/tokenizer.json    # HF
├── kimi-k2/tiktoken.model        # tiktoken (no tokenizer.json upstream)
└── hunyuan/tokenizer.json        # HF (opt-in)
```

The directory name is the `builtin:<key>` used in `config/models.toml`
(e.g. `source = "builtin:qwen3"`).

## How to vendor (recommended: the fetch script)

```bash
scripts/fetch-tokenizers.sh          # qwen3 + deepseek-v3 + kimi-k2 (default set)
scripts/fetch-tokenizers.sh hunyuan  # also fetch the opt-in Hunyuan model
cargo build --release
```

The script pins each file to an exact HuggingFace commit and verifies it against
`checksums.sha256`, so downloads are reproducible and tamper-evident.

### Manual

Download the file and drop it in the matching directory:

```bash
curl -fL -o tokenizers/qwen3/tokenizer.json \
  https://huggingface.co/Qwen/Qwen3-8B/resolve/<commit>/tokenizer.json
```

## Notes

* **Kimi-K2** has no `tokenizer.json`; it uses `tiktoken.model` + a custom split
  pattern. `ctos` loads it via tiktoken-rs with Kimi's original pattern, so the
  counts are accurate and offline. The pattern lives in `src/tokenizer/builtin.rs`.
* If a file is **absent**, the build still succeeds; that `builtin:` model reports
  a clear error at runtime. You can always use a `file:` source instead.
* **Licenses:** each file is redistributed under its own model license, not
  ctos's GPL-3.0. See [`../THIRD_PARTY_LICENSES/README.md`](../THIRD_PARTY_LICENSES/README.md).
* These files are large (~21 MB total). The default set **is committed** to this
  repo so it builds offline out of the box; `checksums.sha256` locks their bytes.
  They live permanently in git history — if you'd rather keep the repo lean, drop
  them from git and fetch at build time instead (the build tolerates their
  absence). Opt-in models like Hunyuan are **not** committed.
* Pin the exact revision — tokenizer changes shift counts, which the golden tests
  are designed to catch.
