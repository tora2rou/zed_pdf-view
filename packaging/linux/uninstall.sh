#!/usr/bin/env bash
# install.sh で入れたTuzuriを消す。設定 (~/.config/tuzuri) とデータ (~/.local/share/tuzuri) は残す。
set -euo pipefail

prefix=${PREFIX:-$HOME/.local}
rm -rf "$prefix/lib/tuzuri"
rm -f "$prefix/bin/tuzuri" "$prefix/share/applications/tuzuri.desktop"
for size in 512x512 1024x1024; do
  rm -f "$prefix/share/icons/hicolor/$size/apps/tuzuri.png"
done
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
echo "Tuzuriを消しました。設定とデータは ~/.config/tuzuri と ~/.local/share/tuzuri に残っています。"
