#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v node >/dev/null && [ -f "$HOME/.nvm/nvm.sh" ]; then
  set +u
  source "$HOME/.nvm/nvm.sh"
  nvm use 22
  set -u
fi
npm ci
npm run typecheck
npm test
CI=true npm run dist:mac -- --ci
version=$(node -p 'require("./package.json").version')
destination="release/v$version"
mkdir -p "$destination"
images=(src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/*.dmg)
[ "${#images[@]}" -eq 1 ]
hdiutil verify "${images[0]}"
for app in src-tauri/target/aarch64-apple-darwin/release/bundle/macos/*.app; do
  codesign --verify --deep --strict "$app"
done
cp "${images[0]}" "$destination/Codex-Proxy-Launcher-v$version-macos-arm64.dmg"
shasum -a 256 "$destination/Codex-Proxy-Launcher-v$version-macos-arm64.dmg"
