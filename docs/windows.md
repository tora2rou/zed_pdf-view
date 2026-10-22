# Windows版 Chihayaburu のビルドとインストール

Chihayaburu は、Windowsでもそのままビルドできる(GPUIがWindowsに対応している)。
ビルドしたexeには、かるたのアイコンが埋め込まれ、起動時にコンソール(黒い画面)は出ない。

## 1. 必要なもの(初回だけ)

1. **Visual Studio 2022 Build Tools**(または Visual Studio 2022)
   - インストーラで「**C++ によるデスクトップ開発**」を選ぶ。MSVC と Windows SDK が入る。
   - GPUI のビルドには、Windows SDK に含まれるシェーダーコンパイラ等が必要。
2. **Rust**: <https://rustup.rs> から `rustup-init.exe` を実行する(既定の `x86_64-pc-windows-msvc` のまま)。
   - このリポジトリの `rust-toolchain.toml` により、必要なバージョン(1.98.1)は自動で入る。
3. **Git for Windows**

## ビルド済みのzipを使う(ビルド不要)

GitHub Actionsが、`windows-exe`ブランチのpushと`v*`タグのたびにexeをビルドする。
[Releases](https://github.com/tora2rou/zed_pdf-view/releases) の `Chihayaburu-windows-x64.zip`(タグのとき)、または Actions の実行結果のアーティファクトからダウンロードして、展開した中で次を実行する(下の「3. インストール」と同じ)。

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

## 2. ビルド

**WSL のフォルダ(`\\wsl.localhost\...`)を直接ビルドしない**(遅く、パスの扱いで失敗することがある)。Windows側のフォルダに clone する。

```powershell
git clone https://github.com/tora2rou/zed_pdf-view.git
cd zed_pdf-view
git switch windows-exe        # main に取り込まれるまでは、このブランチ
cargo build --release -p pdf_view_app
```

- 初回は GPUI のビルドに時間がかかる(数分〜十数分)。
- できあがるのは `target\release\pdf_view_app.exe`。

試しに起動する:

```powershell
.\target\release\pdf_view_app.exe "C:\path\to\file.pdf"
```

## 3. インストール(管理者権限は要らない)

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\install.ps1
```

- exe を `%LOCALAPPDATA%\Programs\Chihayaburu\Chihayaburu.exe` にコピーする。
- スタートメニューに「Chihayaburu」を追加する。
- PDF の「プログラムから開く」と、設定の「既定のアプリ」に登録する。

**PDF をダブルクリックで開くには、最初の1回だけ:** PDF を右クリック →「プログラムから開く」→「別のプログラムを選択」→ Chihayaburu →「常に使う」。
(Windows 10/11 は、アプリが自分で既定のアプリになることを許していないため。)

アップデートするときは、もう一度ビルドしてから `install.ps1` を実行する(Chihayaburu は閉じておく)。

## 4. アンインストール

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\uninstall.ps1
```

最近開いたファイルの記録(`%APPDATA%\pdf_view_app\recent.txt`)は残る。不要なら手で消す。

## 困ったとき

- **ビルドが `link.exe` や `rc.exe`、シェーダーのところで失敗する**: Build Tools の「C++ によるデスクトップ開発」(Windows SDK を含む)が入っているか確認する。
- **ログを見たい**: release版はコンソールを出さないので、ログは見えない。デバッグ版はコンソール付きで起動する:
  ```powershell
  $env:PDF_VIEW_LOG = "debug"
  cargo run -p pdf_view_app -- "C:\path\to\file.pdf"
  ```
- **スクリプトの日本語が化ける・構文エラーになる**: `install.ps1` / `uninstall.ps1` は BOM 付き UTF-8 で保存してある(Windows PowerShell 5.1 は、BOM の無い UTF-8 を Shift_JIS として読むため)。編集するときは BOM を消さないこと。
