#!/usr/bin/env bash
# Tuzuriの実行ファイルを、Linux向けの tar.gz にまとめる。
#
#   packaging/linux/package.sh <実行ファイル> <出力フォルダ> <バージョン>
#
# 中身: bin/tuzuri、lib/ (どのディストリビューションにもある基本のもの以外の共有ライブラリ)、
# アイコン、デスクトップファイル、install.sh / uninstall.sh。
# 実行ファイルは .cargo/bundle-config.toml でビルドする (lib/ を探すように rpath が入る)。
set -euo pipefail

binary=$1
out_dir=$2
version=$3

root=$(cd "$(dirname "$0")/../.." && pwd)
name="tuzuri-${version}-linux-$(uname -m)"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
dir="$stage/$name"

mkdir -p "$dir/bin" "$dir/lib" "$dir/share/applications" \
  "$dir/share/icons/hicolor/512x512/apps" "$dir/share/icons/hicolor/1024x1024/apps"

cp "$binary" "$dir/bin/tuzuri"
strip --strip-unneeded "$dir/bin/tuzuri"

# Zedの script/bundle-linux と同じく、基本のライブラリ以外を同梱する。ただし、次のものはシステムのものを使う。
# - libstdc++: GPUのドライバ (Mesa) が実行時に読み込まれ、システムの新しい libstdc++ を必要とする。
#   ビルドしたUbuntuの古いものが先に読まれていると、ドライバが読めず画面を作れない
#   ("Failed to create surface")。libstdc++ は後方互換なので、新しいシステムのもので動く。
# - libxkbcommon: システムのキーボードの設定ファイルと版が合っていないと、読めずにエラーを出す。
ldd "$binary" | awk '/=> \// { print $3 }' |
  grep -v -E '/(ld-linux[^/]*|libc|libm|libgcc_s|libstdc\+\+|libxkbcommon(-x11)?|libpthread|libdl|libresolv|librt|libasound)\.so' |
  grep -v -E '/(libglib-2\.0|libgobject-2\.0|libgio-2\.0|libgmodule-2\.0|libpcre2?-8?|libmount|libblkid|libselinux|libffi|libz|libsystemd)\.so' |
  while read -r lib; do cp -L "$lib" "$dir/lib/"; done || true

cp "$root/crates/zed/resources/app-icon-dev.png" "$dir/share/icons/hicolor/512x512/apps/tuzuri.png"
cp "$root/crates/zed/resources/app-icon-dev@2x.png" "$dir/share/icons/hicolor/1024x1024/apps/tuzuri.png"
cp "$root/packaging/linux/tuzuri.desktop" "$dir/share/applications/tuzuri.desktop"
cp "$root/packaging/linux/install.sh" "$root/packaging/linux/uninstall.sh" "$dir/"
cp "$root/packaging/linux/README.txt" "$dir/"

mkdir -p "$out_dir"
tar -C "$stage" -czf "$out_dir/$name.tar.gz" "$name"
echo "$out_dir/$name.tar.gz"
