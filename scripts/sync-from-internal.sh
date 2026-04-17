#!/usr/bin/env bash
#
# sync-from-internal.sh — mirror game environments from an internal agent-clash
# checkout into this public repo.
#
# Usage:
#   scripts/sync-from-internal.sh <path-to-internal-agent-clash-checkout>
#
# The script:
#   1. Copies allowlisted crates + services from internal (overwriting).
#   2. Applies rename/decoupling edits (pvp-engine -> environment-engine,
#      environment-client -> environment-server, drops arena-orchestration
#      path-dep, replaces authors = [...] with authors = [], rewrites
#      arena_orchestration imports to eval_runtime).
#   3. Runs a grep gate for banned internal strings; fails loudly on any hit.
#   4. cargo build --workspace && cargo test --workspace.
#   5. Leaves a dirty working tree for the operator to review, commit, push.
#
# Operator responsibility: diff and review before committing. Never push
# without confirming no internal tooling leaked.

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <path-to-internal-agent-clash-checkout>" >&2
  exit 2
fi

SRC="$(cd "$1" && pwd)"
DST="$(cd "$(dirname "$0")/.." && pwd)"

if [[ ! -d "$SRC/crates/pvp-engine" ]]; then
  echo "error: $SRC does not look like an agent-clash checkout (missing crates/pvp-engine)" >&2
  exit 1
fi

echo "[sync] src:  $SRC"
echo "[sync] dst:  $DST"

# Clean any stray .bak files from a previous interrupted run.
trap 'find "$DST" -name "*.bak" -delete 2>/dev/null || true' EXIT

# -------- 1. Copy allowlisted paths --------
CRATES=(
  eval-runtime
  unified-event-protocol
  coup-protocol coup-engine
  vibe-check-protocol vibe-check-engine
  wordle-protocol
  tic-tac-toe-protocol
  connect-four-protocol
  red-button-protocol
  poker-protocol
)

for c in "${CRATES[@]}"; do
  rm -rf "$DST/crates/$c"
  cp -R "$SRC/crates/$c" "$DST/crates/$c"
done

# pvp-engine -> environment-engine
rm -rf "$DST/crates/environment-engine"
cp -R "$SRC/crates/pvp-engine" "$DST/crates/environment-engine"

# services/environment-client -> services/environment-server
rm -rf "$DST/services/environment-server"
cp -R "$SRC/services/environment-client" "$DST/services/environment-server"

# Remove internal history that comes along with the copies.
find "$DST/crates" "$DST/services" \( -name CHANGELOG.md -o -name README.md \) -delete

# -------- 2. Apply decoupling edits --------

# a) drop arena-orchestration path-dep from every game protocol Cargo.toml.
for f in \
  "$DST/crates/coup-protocol/Cargo.toml" \
  "$DST/crates/vibe-check-protocol/Cargo.toml" \
  "$DST/crates/wordle-protocol/Cargo.toml" \
  "$DST/crates/tic-tac-toe-protocol/Cargo.toml" \
  "$DST/crates/connect-four-protocol/Cargo.toml" \
  "$DST/crates/red-button-protocol/Cargo.toml" \
  "$DST/crates/poker-protocol/Cargo.toml"
do
  # Strip the arena-orchestration line.
  grep -v 'arena-orchestration = { path = "../arena-orchestration" }' "$f" > "$f.tmp"
  mv "$f.tmp" "$f"
done

# b) add eval-runtime path-dep where missing (idempotent).
for c in coup-protocol vibe-check-protocol wordle-protocol tic-tac-toe-protocol \
         connect-four-protocol red-button-protocol poker-protocol
do
  f="$DST/crates/$c/Cargo.toml"
  if ! grep -q 'eval-runtime = { path = "../eval-runtime" }' "$f"; then
    # Insert right after [dependencies] header.
    awk '
      /^\[dependencies\]/ { print; print "eval-runtime = { path = \"../eval-runtime\" }"; next }
      { print }
    ' "$f" > "$f.tmp"
    mv "$f.tmp" "$f"
  fi
done

# c) replace authors fields (cover the common internal value).
find "$DST/crates" "$DST/services" -name Cargo.toml -print0 | while IFS= read -r -d '' f; do
  sed -i.bak 's/authors = \["AgentClash Team"\]/authors = []/' "$f"
  rm -f "$f.bak"
done

# d) rewrite arena_orchestration imports -> eval_runtime.
grep -rl 'use arena_orchestration::' "$DST/crates" "$DST/services" 2>/dev/null | while read -r f; do
  sed -i.bak 's/use arena_orchestration::/use eval_runtime::/g' "$f"
  rm -f "$f.bak"
done

# e) rename environment-engine crate metadata + uses.
sed -i.bak 's/^name = "pvp-engine"/name = "environment-engine"/' "$DST/crates/environment-engine/Cargo.toml"
rm -f "$DST/crates/environment-engine/Cargo.toml.bak"
grep -rl 'pvp_engine' "$DST/crates/environment-engine" "$DST/services" 2>/dev/null | while read -r f; do
  sed -i.bak 's/pvp_engine/environment_engine/g' "$f"
  rm -f "$f.bak"
done

# f) rename environment-server crate metadata + uses.
f="$DST/services/environment-server/Cargo.toml"
sed -i.bak \
  -e 's/^name = "environment-client-service"/name = "environment-server"/' \
  -e 's|^name = "environment_client_service"|name = "environment_server"|' \
  -e 's|^name = "environment-client-service"|name = "environment-server"|' \
  "$f"
rm -f "$f.bak"
grep -rl 'environment_client_service' "$DST/services" 2>/dev/null | while read -r f; do
  sed -i.bak 's/environment_client_service/environment_server/g' "$f"
  rm -f "$f.bak"
done

# g) rewrite pvp-engine path-dep -> environment-engine in environment-server/Cargo.toml.
sed -i.bak 's|pvp-engine = { path = "../../crates/pvp-engine"|environment-engine = { path = "../../crates/environment-engine"|' \
  "$DST/services/environment-server/Cargo.toml"
rm -f "$DST/services/environment-server/Cargo.toml.bak"

# -------- 3. Grep gate --------
BANNED='agent-clash|ClashAI|clashai|agent_clash|agentclash|game_arena|game-arena|arena-orchestration|arena_orchestration|MatchOrchestrator|TurnOrchestrator|auth0|sentry|clashai-production|opentelemetry|llm-client|llm_client|agent-harness|agent_harness|polymarket|lmsr|wager|Co-Authored-By: Claude|\.claude/|CLAUDE\.md|moltpvp|taso-ventures|AgentClash'

HITS=$(grep -rnE -i "$BANNED" \
  --include="*.rs" --include="*.toml" --include="*.md" --include="*.html" \
  --include="*.json" --include="*.js" --include="*.css" --include="Dockerfile*" \
  --include="*.yml" --include="*.sh" \
  "$DST" 2>/dev/null | grep -v "/target/" | grep -v "/.git/" || true)

if [[ -n "$HITS" ]]; then
  echo "[sync] FAIL: banned strings still present:" >&2
  echo "$HITS" >&2
  exit 1
fi

# -------- 4. Build + lint + test --------
pushd "$DST" > /dev/null
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
popd > /dev/null

echo "[sync] OK. Review the diff, then commit and push."
