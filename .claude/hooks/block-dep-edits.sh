#!/bin/bash
set -uo pipefail

# Fail closed: if jq is missing, deny the edit rather than allowing it
if ! command -v jq &>/dev/null; then
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Hook dependency (jq) missing; edit blocked as precaution."}}\n'
  exit 0
fi

INPUT=$(cat)
FILE_PATH=$(jq -r '.tool_input.file_path // empty' <<< "$INPUT") || exit 0
FILENAME=$(basename "$FILE_PATH")

case "$FILENAME" in
  Cargo.toml)
    REASON="Use 'cargo add/rm/upgrade' instead of editing Cargo.toml directly." ;;
  Cargo.lock)
    REASON="Use 'cargo add/rm/upgrade' instead of editing Cargo.lock directly." ;;
  pnpm-lock.yaml)
    REASON="Use 'pnpm add/remove/update' instead of editing pnpm-lock.yaml." ;;
  package.json)
    REASON="Use 'pnpm add/remove/update' instead of editing package.json." ;;
  .env|.env.*)
    REASON="Do not edit environment files — they may contain secrets." ;;
  *)
    exit 0 ;;
esac

jq -n --arg reason "$REASON" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: $reason
  }
}'
exit 0
