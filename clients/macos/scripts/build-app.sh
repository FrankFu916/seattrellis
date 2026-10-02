#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "The SwiftUI preview must be built on macOS with Xcode Command Line Tools." >&2
  exit 1
fi

client_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$client_root/../.." && pwd)"
native_lib_dir="${SEATTRELLIS_NATIVE_LIB_DIR:-$repo_root/target/release}"
product_version="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([0-9][0-9.]*\)"[[:space:]]*$/\1/p' "$repo_root/crates/seattrellis-native-bridge/Cargo.toml")"
if [[ ! "$product_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "The Rust bridge must declare a numeric major.minor.patch product version." >&2
  exit 1
fi

if [[ ! -f "$native_lib_dir/libseattrellis_bridge.a" ]]; then
  echo "Build the Rust bridge first: cargo build --release --locked -p seattrellis-native-bridge" >&2
  exit 1
fi

export SEATTRELLIS_NATIVE_LIB_DIR="$native_lib_dir"
swift build --package-path "$client_root" --configuration release
binary_dir="$(swift build --package-path "$client_root" --configuration release --show-bin-path)"
app_dir="$client_root/build/SeatTrellis.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$binary_dir/SeatTrellisMac" "$app_dir/Contents/MacOS/SeatTrellis"
dynamic_dependencies="$(otool -L "$app_dir/Contents/MacOS/SeatTrellis")"
case "$dynamic_dependencies" in
  *libseattrellis_bridge*|*target/release*)
    echo "The preview must link the Rust archive without a checkout-dependent dylib." >&2
    exit 1
    ;;
esac
cat > "$app_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>org.seattrellis.macos.preview</string>
  <key>CFBundleName</key><string>SeatTrellis</string>
  <key>CFBundleDisplayName</key><string>SeatTrellis Native Preview</string>
  <key>CFBundleExecutable</key><string>SeatTrellis</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticTermination</key><false/>
</dict></plist>
PLIST
plutil -replace CFBundleShortVersionString -string "$product_version" "$app_dir/Contents/Info.plist"
plutil -lint "$app_dir/Contents/Info.plist"
echo "Built unsigned native preview: $app_dir"
