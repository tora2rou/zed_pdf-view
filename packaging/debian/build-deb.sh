#!/usr/bin/env bash
# Chihayaburu の .deb を作る(Debian / Ubuntu / Mint など)。
#
#   packaging/debian/build-deb.sh [出力先のフォルダ]     # 既定は ./dist
#
# 必要なもの: Rust(rustup)、dpkg-dev、binutils、fakeroot不要(dpkg-deb --root-owner-group を使う)。
# GPUIのビルドに必要なパッケージは docs/linux.md を参照。
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out_dir="$(realpath -m "${1:-$root/dist}")"

command -v dpkg-deb >/dev/null || { echo "dpkg-deb がありません(dpkg-dev を入れてください)" >&2; exit 1; }
arch="$(dpkg --print-architecture)"
version="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version *= *"\(.*\)"/\1/p}' "$root/Cargo.toml" | head -n1)"
[ -n "$version" ] || { echo "Cargo.toml からバージョンを読めません" >&2; exit 1; }

echo "==> ビルド (release, locked)"
(cd "$root" && cargo build --release --locked -p pdf_view_app)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
stage="$work/pkg"

echo "==> ステージング: $stage"
make -C "$root/packaging/linux" install DESTDIR="$stage" PREFIX=/usr >/dev/null
strip --strip-unneeded "$stage/usr/bin/chihayaburu"

# --- ドキュメント(Debianのポリシー: copyright と changelog は必須) ---
doc="$stage/usr/share/doc/chihayaburu"
install -d "$doc"
cat >"$doc/copyright" <<COPYRIGHT
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: Chihayaburu
Source: https://github.com/tora2rou/zed_pdf-view

Files: *
Copyright: 2026 tora2rou
License: Apache-2.0

License: Apache-2.0
 On Debian systems, the full text of the Apache License 2.0 can be found in
 /usr/share/common-licenses/Apache-2.0.
 .
 The binary statically links third-party Rust crates (GPUI, hayro, and others),
 each under its own license; see their respective source packages.
COPYRIGHT
{
  echo "chihayaburu ($version) unstable; urgency=low"
  echo
  echo "  * Release $version."
  echo
  echo " -- tora2rou <tora2rou@users.noreply.github.com>  $(date -R)"
} | gzip -9n >"$doc/changelog.Debian.gz"

# --- 依存関係: リンクされているライブラリは dpkg-shlibdeps で調べる ---
# dpkg-shlibdeps は debian/control を要求するので、仮のものを置く。
shl="$work/shlibs"
mkdir -p "$shl/debian"
printf 'Source: chihayaburu\n\nPackage: chihayaburu\nArchitecture: any\n' >"$shl/debian/control"
linked="$(cd "$shl" && dpkg-shlibdeps -O -e"$stage/usr/bin/chihayaburu" 2>/dev/null | sed 's/^shlibs:Depends=//')"

# 実行時に dlopen されるもの(ldd には出ない): Wayland、Vulkan(描画)
runtime="libwayland-client0, libvulkan1"
depends="$linked, $runtime"

installed_size="$(du -sk "$stage" | cut -f1)"
install -d "$stage/DEBIAN"
cat >"$stage/DEBIAN/control" <<CONTROL
Package: chihayaburu
Version: $version
Section: graphics
Priority: optional
Architecture: $arch
Depends: $depends
Recommends: xdg-desktop-portal, xdg-desktop-portal-gtk | xdg-desktop-portal-gnome | xdg-desktop-portal-kde | xdg-desktop-portal-wlr, xdg-utils, mesa-vulkan-drivers
Installed-Size: $installed_size
Maintainer: tora2rou <tora2rou@users.noreply.github.com>
Homepage: https://github.com/tora2rou/zed_pdf-view
Description: Fast, simple PDF viewer
 Chihayaburu is a PDF viewer built with GPUI. It scrolls smoothly through
 all pages, zooms, rotates, and shows page thumbnails and the table of
 contents. Text can be selected and copied, and links (including URLs written
 as plain text) can be clicked.
 .
 The file dialog needs an xdg-desktop-portal backend, and links are opened
 with xdg-open.
CONTROL

mkdir -p "$out_dir"
deb="$out_dir/chihayaburu_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
echo "==> できました: $deb"
dpkg-deb --info "$deb" | sed -n '/Package:/,/Description:/p' | head -12
