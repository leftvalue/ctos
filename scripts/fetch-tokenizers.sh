#!/usr/bin/env bash
# Fetch and verify the tokenizer files that ctos embeds as `builtin:` models.
#
# Files are pinned to exact HuggingFace commit revisions and verified against
# tokenizers/checksums.sha256, so downloads are reproducible and tamper-evident.
# After a successful run, `cargo build --release` produces a fully offline binary.
#
# Usage:
#   scripts/fetch-tokenizers.sh            # fetch the default set (qwen3, deepseek-v3, kimi-k2)
#   scripts/fetch-tokenizers.sh hunyuan    # additionally fetch opt-in models (see below)
#
# NOTE ON LICENSES: each file is redistributed under its own model license, not
# ctos's GPL-3.0. See THIRD_PARTY_LICENSES/README.md. Hunyuan is opt-in because
# its community license carries extra redistribution conditions — review before use.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOK_DIR="$ROOT/tokenizers"
HF="${HF_ENDPOINT:-https://huggingface.co}"

# key | repo | revision (commit sha) | filename | dest filename
# Default set: Apache-2.0 / MIT-family licenses (clean to redistribute).
read -r -d '' DEFAULT_MODELS <<'EOF' || true
qwen3|Qwen/Qwen3-8B|b968826d9c46dd6066d109eabc6255188de91218|tokenizer.json|tokenizer.json
deepseek-v3|deepseek-ai/DeepSeek-V3|e815299b0bcbac849fa540c768ef21845365c9eb|tokenizer.json|tokenizer.json
kimi-k2|moonshotai/Kimi-K2-Instruct|fd1984e2b7a3350dbf7305fe73a4ede25c14de50|tiktoken.model|tiktoken.model
EOF

# Opt-in models (fetched only when named on the command line).
# Hunyuan: Tencent Hunyuan Community License — review its terms before shipping.
read -r -d '' OPTIN_MODELS <<'EOF' || true
hunyuan|tencent/Hunyuan-A13B-Instruct|main|tokenizer.json|tokenizer.json
EOF

fetch_one() {
  local line="$1"
  IFS='|' read -r key repo rev fname dest <<<"$line"
  local url="$HF/$repo/resolve/$rev/$fname"
  local out_dir="$TOK_DIR/$key"
  local out="$out_dir/$dest"
  mkdir -p "$out_dir"
  echo ">> $key: $repo@${rev:0:12} :: $fname"
  curl -fSL --retry 3 -o "$out" "$url"
}

selected=("$@")
echo "Fetching default tokenizers into $TOK_DIR ..."
while IFS= read -r line; do
  [ -z "$line" ] && continue
  fetch_one "$line"
done <<<"$DEFAULT_MODELS"

for want in "${selected[@]:-}"; do
  [ -z "$want" ] && continue
  while IFS= read -r line; do
    [ -z "$line" ] && continue
    IFS='|' read -r key _ <<<"$line"
    if [ "$key" = "$want" ]; then fetch_one "$line"; fi
  done <<<"$OPTIN_MODELS"
done

echo
echo "Verifying checksums ..."
# Verify only the lines whose files exist (opt-in files may be absent).
verify_input="$( cd "$ROOT" && grep -v '^#' tokenizers/checksums.sha256 | while read -r sum path; do
    [ -z "$sum" ] && continue
    if [ -f "$path" ]; then echo "$sum  $path"; fi
  done )"
if command -v sha256sum >/dev/null 2>&1; then
  ( cd "$ROOT" && printf '%s\n' "$verify_input" | sha256sum -c - )
elif command -v shasum >/dev/null 2>&1; then
  ( cd "$ROOT" && printf '%s\n' "$verify_input" | shasum -a 256 -c - )
else
  echo "warning: no sha256sum/shasum found; skipping verification" >&2
fi

echo
echo "Done. Now run: cargo build --release"
