# Third-party tokenizer licenses

`ctos` can embed tokenizer files from several open models as `builtin:` sources.
Each file is redistributed **under its own model license — not ctos's GPL-3.0**.
Embedding a tokenizer into the `ctos` binary is a form of redistribution, so if
you build and distribute a binary with these bundled, you must comply with and
carry the corresponding license.

The default set below is **committed into this repository** (so it builds fully
offline), and each model's upstream license text is stored in this directory.

## Default set (bundled by the fetch script)

| builtin key | model | file | source repo (pinned) | license |
|---|---|---|---|---|
| `qwen3` | Qwen3-8B | `tokenizer.json` | `Qwen/Qwen3-8B` @ `b968826…` | Apache-2.0 |
| `deepseek-v3` | DeepSeek-V3 | `tokenizer.json` | `deepseek-ai/DeepSeek-V3` @ `e815299…` | DeepSeek (code MIT; see repo `LICENSE`) |
| `kimi-k2` | Kimi-K2-Instruct | `tiktoken.model` | `moonshotai/Kimi-K2-Instruct` @ `fd1984e…` | Modified MIT |

Kimi-K2 does not ship a HuggingFace `tokenizer.json`; it uses a `tiktoken.model`
BPE vocab plus a custom split pattern. `ctos` loads it via tiktoken-rs using the
model's original pattern, so counts match the model exactly and remain offline.

### License files in this directory

| model | file(s) |
|---|---|
| Qwen3 | `qwen3-LICENSE.txt` (Apache-2.0) |
| DeepSeek-V3 | `deepseek-v3-LICENSE-CODE.txt` (MIT), `deepseek-v3-LICENSE-MODEL.txt` (DeepSeek License Agreement) |
| Kimi-K2 | `kimi-k2-LICENSE.txt` (Modified MIT) |

The `tokenizer.json` / `tiktoken.model` artifacts are code/config assets; the
MIT-family terms above permit their redistribution provided the license text is
retained (which is why it lives here).

## Opt-in models

| builtin key | model | license | note |
|---|---|---|---|
| `hunyuan` | Tencent Hunyuan | Tencent Hunyuan Community License | **Review before redistributing.** Has additional conditions (attribution, use/geographic restrictions). Fetched only via `scripts/fetch-tokenizers.sh hunyuan`, and **not committed** to this repo. |

## Maintenance

1. The default set is committed; `scripts/fetch-tokenizers.sh` can re-fetch and
   verify them against `tokenizers/checksums.sha256`.
2. When bumping a pinned revision, refresh both the file and its license text
   here, and update the attribution table above.
3. If you cannot satisfy a model's license terms, do **not** bundle it — leave it
   out and let end users supply it via a `file:` source at runtime.

> Licenses change between revisions. Always confirm the license text at the exact
> commit you pin, not just the model card.
