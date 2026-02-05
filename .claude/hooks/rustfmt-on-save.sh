#!/bin/bash
INPUT=$(cat)
FILE_PATH=$(jq -r '.tool_input.file_path // empty' <<< "$INPUT") || exit 0
if [[ "$FILE_PATH" == *.rs && -f "$FILE_PATH" ]]; then
  if ! rustfmt "$FILE_PATH" 2>&1; then
    echo "warning: rustfmt failed on $FILE_PATH" >&2
  fi
fi
exit 0
