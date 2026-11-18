#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
project_dir=$(pwd -P)
target_dir="$project_dir/target"
app_name='ChatGPT Proxy Launcher.app'
output_dir="$target_dir/macos"
output_app="$output_dir/$app_name"
native_target=$(rustc -vV | sed -n 's/^host: //p')

cargo build --release --locked --target "$native_target" --target-dir "$target_dir"

mkdir -p "$output_dir"
staging_dir=$(mktemp -d "$output_dir/.package.XXXXXX")
trap 'rm -rf "$staging_dir"' EXIT
staged_app="$staging_dir/$app_name"
mkdir -p "$staged_app/Contents/MacOS" "$staged_app/Contents/Resources"
install -m 644 macos/Info.plist "$staged_app/Contents/Info.plist"
install -m 644 macos/AppIcon.icns "$staged_app/Contents/Resources/AppIcon.icns"
install -m 644 LICENSE "$staged_app/Contents/Resources/LICENSE"
install -m 755 "$target_dir/$native_target/release/chatgpt-app-launcher" "$staged_app/Contents/MacOS/chatgpt-app-launcher"
/usr/bin/plutil -lint "$staged_app/Contents/Info.plist"
/usr/bin/codesign --sign - "$staged_app"
/usr/bin/codesign --verify --strict "$staged_app"

if [ -e "$output_app" ]; then
    mv "$output_app" "$staging_dir/previous.app"
fi
if ! mv "$staged_app" "$output_app"; then
    if [ -d "$staging_dir/previous.app" ]; then
        if ! mv "$staging_dir/previous.app" "$output_app"; then
            trap - EXIT
            printf 'Previous app is at %s\n' "$staging_dir/previous.app" >&2
        fi
    fi
    exit 1
fi

printf '%s\n' "$output_app"
