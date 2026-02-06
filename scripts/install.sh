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

# Track whether GUI app was running, so we can relaunch after install.
APP_WAS_RUNNING=false
if pgrep -x "$BINARY_NAME" >/dev/null 2>&1; then
    APP_WAS_RUNNING=true
fi

# Check if daemon was running (to restart it after install)
DAEMON_WAS_RUNNING=false
if "$BUILT" daemon status >/dev/null 2>&1; then
    DAEMON_WAS_RUNNING=true
    info "Stopping daemon..."
    "$BUILT" daemon stop || true
    sleep 0.5
fi

# Kill any other running instances after build so the binary isn't busy during copy
# (keeps the app usable during the build)
if pgrep -x "$BINARY_NAME" >/dev/null 2>&1; then
    info "Stopping running ${BINARY_NAME} instances..."
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

# Relaunch GUI if it was running before install.
if [[ "$APP_WAS_RUNNING" == "true" ]]; then
    info "Relaunching ${BINARY_NAME}..."
    nohup "$INSTALL_PATH" >/dev/null 2>&1 &
fi

# Restart daemon if it was running before
RESTART_FAILED=false
if [[ "$DAEMON_WAS_RUNNING" == "true" ]]; then
    info "Restarting daemon..."
    "$INSTALL_PATH" daemon start --background
    # Retry status check with timeout
    for i in {1..5}; do
        sleep 0.5
        if "$INSTALL_PATH" daemon status >/dev/null 2>&1; then
            info "Daemon restarted successfully"
            break
        fi
        if [[ $i -eq 5 ]]; then
            error "Failed to restart daemon — start manually with: ${BINARY_NAME} daemon"
            RESTART_FAILED=true
        fi
    done
fi

if [[ "$RESTART_FAILED" == "true" ]]; then
    info "Binary installed to ${INSTALL_PATH} (daemon restart failed)"
    exit 1
else
    info "Installed successfully. Run with: ${BINARY_NAME}"
fi
