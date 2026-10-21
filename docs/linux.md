# Linux版 Chihayaburu のインストール

Debian系(Debian・Ubuntu・Mintなど)、Arch系(Arch・Manjaroなど)、Nix(NixOSを含む)に対応する。
どの方法でも、次のものが入る(`chihayaburu` というコマンド、アプリ一覧のアイコンと名前、PDFを開く関連付け)。

| 入るもの | 場所(`/usr`の場合) |
|---|---|
| 実行ファイル | `/usr/bin/chihayaburu` |
| デスクトップファイル | `/usr/share/applications/io.github.tora2rou.Chihayaburu.desktop` |
| アプリのメタ情報(AppStream) | `/usr/share/metainfo/io.github.tora2rou.Chihayaburu.metainfo.xml` |
| アイコン(16〜256px) | `/usr/share/icons/hicolor/*/apps/chihayaburu.png` |

## 実行に必要なもの(どの方法でも共通)

- **Vulkan対応のGPUドライバ**(描画に使う)。Mesaの `mesa-vulkan-drivers`(Debian系)、`vulkan-radeon` / `vulkan-intel` / `nvidia-utils`(Arch系)など。
- **xdg-desktop-portal と、その実装**(`xdg-desktop-portal-gtk`、`-gnome`、`-kde`、`-wlr` のどれか)。**ファイルを開くダイアログ**(`Ctrl+O`、`Open`ボタン)に使う。無いと、ダイアログが開けない(エラーが画面の上部に出る。ドラッグ&ドロップと`Recent`は使える)。WSLg でも同じ。
- **xdg-utils**(`xdg-open`)。リンクをブラウザで開くのに使う。

## Debian / Ubuntu / Mint(.deb)

`.deb` はこのリポジトリから作る。**インストールしたいディストリビューションの上で作ること**(下の注意を参照)。

```sh
# 必要なもの(ビルド用): Rust(rustup)、build-essential、clang、cmake、pkg-config、dpkg-dev
git clone https://github.com/tora2rou/zed_pdf-view.git
cd zed_pdf-view
packaging/debian/build-deb.sh            # dist/chihayaburu_<version>_<arch>.deb ができる
sudo apt install ./dist/chihayaburu_*.deb
```

アンインストール: `sudo apt remove chihayaburu`

**注意**: `.deb` の依存関係(特に `libc6 (>= ...)`)は、ビルドしたマシンのライブラリから自動で決まる。Ubuntu 24.04 で作ったものは Debian 12 や Ubuntu 22.04 には入らない。古いディストリビューションに入れたいときは、そのディストリビューション(コンテナでもよい)の上でビルドする。

## Arch / Manjaro(PKGBUILD)

```sh
git clone https://github.com/tora2rou/zed_pdf-view.git
cd zed_pdf-view/packaging/arch
makepkg -si
```

- ブランチを指定するとき: `_branch=<ブランチ名> makepkg -si`
- `rust` は 1.98 以上が必要(`rust-toolchain.toml` が 1.98.1 を指定しているため)。`rustup` で入れている場合は、`rustup` が自動で合わせる。
- AUR にはまだ登録していない。

## Nix / NixOS(flake)

```sh
# 入れずに試す
nix run github:tora2rou/zed_pdf-view -- /path/to/file.pdf

# ユーザーの環境に入れる
nix profile install github:tora2rou/zed_pdf-view

# 開発用のシェル(Rustと、実行に必要なライブラリが入る)
nix develop
```

NixOS の設定に入れるときは、flake の入力に追加して、`environment.systemPackages` に `inputs.chihayaburu.packages.${system}.default` を足す。

- `rust-overlay` で `rust-toolchain.toml` のRust(1.98.1)を使う。
- `Cargo.lock` の git の依存(GPUIなど)は `allowBuiltinFetchGit` で取得する。
- 起動スクリプトが、Wayland・Vulkan・フォントのライブラリと `xdg-open` の場所を設定する(実行時に読み込まれるため)。
- GPUのドライバとポータルは、NixOS の設定(`hardware.graphics.enable = true`、`xdg.portal.enable = true` など)で用意する。

## どれでもない場合(`make install`)

```sh
cargo build --release -p pdf_view_app
sudo make -C packaging/linux install PREFIX=/usr/local
# 取り除く
sudo make -C packaging/linux uninstall PREFIX=/usr/local
```

`DESTDIR` も使える(パッケージを作るとき用)。インストール後、アプリ一覧やアイコンが更新されないときは、`update-desktop-database` と `gtk-update-icon-cache` を実行する(多くの環境では自動で更新される)。

## PDFを、このアプリで開くようにする

アプリ一覧の「Chihayaburu」から起動できる。PDFを既定で開くアプリにするには:

```sh
xdg-mime default io.github.tora2rou.Chihayaburu.desktop application/pdf
```

(ファイルマネージャーの「別のアプリで開く」→「常に使う」でもよい。)

## ログを見たいとき

```sh
PDF_VIEW_LOG=debug chihayaburu file.pdf
```

## 確認できていること・できていないこと

2026-10 時点、作者の環境(WSL2 の Ubuntu 24.04)で:

| | 状況 |
|---|---|
| `.deb` のビルド、中身(所有者・権限・依存関係)、展開した実行ファイルの起動、`.desktop`・AppStream の検証 | 確認済み |
| `make install` / `uninstall`(`DESTDIR` に入れて、アンインストールで空になる) | 確認済み |
| PKGBUILD の `pkgver` / `prepare` / `build` / `package` を、`makepkg` と同じ順に実行(`makepkg` 自体は無い) | 確認済み(下の注) |
| `apt install` で実際にインストール、Debian本体での動作 | **未確認**(root権限での導入と、別のディストリビューションが必要) |
| Archでの `makepkg`、Nixの評価とビルド、NixOSでの動作 | **未確認**(その環境が無い) |

動かなかったときは、エラーメッセージを教えてください。
