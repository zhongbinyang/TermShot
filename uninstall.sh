#!/usr/bin/env bash
set -euo pipefail

BUNDLE_ID="com.termshot.app"
PLIST="$HOME/Library/LaunchAgents/${BUNDLE_ID}.plist"
APP_DIR="$HOME/Applications/TermShot.app"
DATA_DIR="$HOME/Library/Application Support/TermShot"

if [ -f "$PLIST" ]; then
    launchctl unload -w "$PLIST" >/dev/null 2>&1 || true
    rm -f "$PLIST"
fi

pkill -x TermShot >/dev/null 2>&1 || true
sleep 0.3

rm -rf "$APP_DIR" "$DATA_DIR"
echo "TermShot uninstalled."
