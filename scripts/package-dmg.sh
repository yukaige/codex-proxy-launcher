#!/bin/zsh
set -euo pipefail

project_root="${0:A:h:h}"
app_path="$project_root/release/mac-arm64/Codex 代理启动器.app"
stage_path="$project_root/release/dmg-root"
dmg_path="$project_root/release/Codex-代理启动器-0.1.1-arm64.dmg"

if [[ ! -d "$app_path" ]]; then
  print -u2 "找不到已构建的应用：$app_path"
  exit 1
fi

codesign --force --deep --sign - "$app_path"
mkdir -p "$stage_path"
ditto "$app_path" "$stage_path/Codex 代理启动器.app"

if [[ ! -e "$stage_path/Applications" ]]; then
  ln -s /Applications "$stage_path/Applications"
fi

hdiutil create \
  -volname "Codex 代理启动器 0.1.1" \
  -srcfolder "$stage_path" \
  -ov \
  -format UDZO \
  "$dmg_path"

codesign --verify --deep --strict --verbose=2 "$app_path"
print "已生成：$dmg_path"
