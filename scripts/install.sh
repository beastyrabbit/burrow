#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="burrow"
INSTALL_DIR="/usr/local/bin"
INSTALL_PATH="${INSTALL_DIR}/${BINARY_NAME}"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

info()  { printf '\033[1;34m==> %s\033[0m\n' "$*"; }
error() { printf '\033[1;31mERROR: %s\033[0m\n' "$*" >&2; }

cd "$PROJECT_DIR"

# Build with tauri (embeds frontend) but skip bundling (no deb/rpm/appimage)
info "Building ${BINARY_NAME}..."
pnpm tauri build --no-bundle

BUILT="${PROJECT_DIR}/src-tauri/target/release/${BINARY_NAME}"
if [[ ! -f "$BUILT" ]]; then
    error "Build failed — binary not found at ${BUILT}"
    exit 1
fi

# Kill running instances after build so the binary isn't busy during copy
# (keeps the app usable during the build)
if pgrep -x "$BINARY_NAME" >/dev/null 2>&1; then
    info "Stopping running ${BINARY_NAME}..."
    pkill -x "$BINARY_NAME" || true
    sleep 0.5
    # Escalate to SIGKILL if graceful shutdown didn't work within 0.5s
    if pgrep -x "$BINARY_NAME" >/dev/null 2>&1; then
        pkill -9 -x "$BINARY_NAME" || true
        sleep 0.3
    fi
fi

# Install binary
info "Installing to ${INSTALL_PATH} (requires sudo)..."
sudo cp "$BUILT" "$INSTALL_PATH"
sudo chmod 755 "$INSTALL_PATH"

info "Installed successfully. Run with: ${BINARY_NAME}"
