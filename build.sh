#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

echo "[1/5] Generating macOS app icon..."
cargo run --release -- --write-icon "$ROOT/assets/AppIcon.icns"

echo "[2/5] Building release binary..."
cargo build --release

OUT="$ROOT/dist"
APP="$OUT/TermShot.app"
CONTENTS="$APP/Contents"
MACOS_DIR="$CONTENTS/MacOS"
RES_DIR="$CONTENTS/Resources"

rm -rf "$OUT"
mkdir -p "$MACOS_DIR" "$RES_DIR"

echo "[3/5] Packaging TermShot.app..."
cp "$ROOT/target/release/TermShot" "$MACOS_DIR/TermShot"
chmod +x "$MACOS_DIR/TermShot"
cp "$ROOT/assets/AppIcon.icns" "$RES_DIR/AppIcon.icns"

cat > "$CONTENTS/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleExecutable</key>
    <string>TermShot</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.termshot.app</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>TermShot</string>
    <key>CFBundleDisplayName</key>
    <string>TermShot</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSScreenCaptureUsageDescription</key>
    <string>TermShot 需要屏幕录制权限以截取选区、窗口和滚动长图。</string>
</dict>
</plist>
EOF

echo "[4/5] Ad-hoc signing TermShot.app..."
codesign --force --deep --sign - "$APP"

cp "$ROOT/install.sh" "$OUT/install.sh"
cp "$ROOT/uninstall.sh" "$OUT/uninstall.sh"
chmod +x "$OUT/install.sh" "$OUT/uninstall.sh"

echo "[5/5] Creating TermShot-macOS.zip..."
ditto -c -k --keepParent "$APP" "$OUT/TermShot-macOS.zip"

echo ""
echo "App Bundle: $APP"
echo "Archive:    $OUT/TermShot-macOS.zip"
