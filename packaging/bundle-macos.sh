#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
bundle="dist/Larpcord.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "${CARGO_TARGET_DIR:-target}/release/larpcord" "$bundle/Contents/MacOS/larpcord"
cp packaging/macos/Info.plist "$bundle/Contents/Info.plist"
cp assets/Larpcord.icns "$bundle/Contents/Resources/Larpcord.icns"
cp -R LICENSE THIRD_PARTY_NOTICES.md licenses "$bundle/Contents/Resources/"
codesign --force --deep --sign - "$bundle"
