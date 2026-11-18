#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

if [ -x "$HERE/TermShot.app/Contents/MacOS/TermShot" ]; then
    exec "$HERE/TermShot.app/Contents/MacOS/TermShot" --install "$@"
elif [ -x "$HERE/dist/TermShot.app/Contents/MacOS/TermShot" ]; then
    exec "$HERE/dist/TermShot.app/Contents/MacOS/TermShot" --install "$@"
elif [ -x "$HERE/TermShot" ]; then
    exec "$HERE/TermShot" --install "$@"
elif [ -x "$HERE/target/release/TermShot" ]; then
    exec "$HERE/target/release/TermShot" --install "$@"
else
    echo "TermShot.app not found. Run ./build.sh first." >&2
    exit 1
fi
