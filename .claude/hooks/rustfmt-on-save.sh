#!/bin/bash
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
if [[ "$FILE_PATH" == *.rs && -f "$FILE_PATH" ]]; then
  rustfmt "$FILE_PATH" 2>/dev/null
fi
exit 0
