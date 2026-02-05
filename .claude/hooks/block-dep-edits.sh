#!/bin/bash
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
FILENAME=$(basename "$FILE_PATH")

case "$FILENAME" in
  Cargo.toml)
    REASON="Use 'cargo add/rm/upgrade' instead of editing Cargo.toml directly." ;;
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
