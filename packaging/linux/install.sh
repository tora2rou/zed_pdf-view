#!/usr/bin/env bash
# Tuzuriを入れる。既定は自分のホームの下 (~/.local)。PREFIX=/usr/local などで変えられる。
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-$HOME/.local}
app_dir="$prefix/lib/tuzuri"

mkdir -p "$app_dir" "$prefix/bin" "$prefix/share/applications" \
  "$prefix/share/icons/hicolor/512x512/apps" "$prefix/share/icons/hicolor/1024x1024/apps"
rm -rf "$app_dir/bin" "$app_dir/lib"
cp -r "$here/bin" "$here/lib" "$app_dir/"
ln -sf "$app_dir/bin/tuzuri" "$prefix/bin/tuzuri"

for size in 512x512 1024x1024; do
  cp "$here/share/icons/hicolor/$size/apps/tuzuri.png" "$prefix/share/icons/hicolor/$size/apps/tuzuri.png"
done
sed "s|@EXEC@|$app_dir/bin/tuzuri|g" "$here/share/applications/tuzuri.desktop" \
  > "$prefix/share/applications/tuzuri.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true

echo "Tuzuriを $app_dir に入れました。"
case ":$PATH:" in
  *":$prefix/bin:"*) echo "端末からは tuzuri で起動できます。" ;;
  *) echo "端末から起動するには、$prefix/bin を PATH に追加してください。" ;;
esac
