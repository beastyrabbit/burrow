#!/usr/bin/env bash
# Clone a dependency repo into examples/ for local reference.
# Usage: ./scripts/clone-examples.sh https://github.com/org/repo.git
set -euo pipefail

if [ $# -eq 0 ]; then
  echo "Usage: $0 <git-url> [<git-url> ...]"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLES_DIR="$SCRIPT_DIR/../examples"
mkdir -p "$EXAMPLES_DIR"

for repo in "$@"; do
  name="$(basename "$repo" .git)"
  dest="$EXAMPLES_DIR/$name"
  if [ -d "$dest" ]; then
    echo "Skipping $name (already exists)"
  else
    echo "Cloning $name..."
    git clone --depth 1 "$repo" "$dest"
  fi
done
