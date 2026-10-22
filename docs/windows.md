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

## OCRを使うとき(任意): Tesseractのインストール

画像だけのPDFの文字を選択・コピーする`OCR`ボタンは、外部コマンドの[Tesseract](https://github.com/tesseract-ocr/tesseract)を使う。使わないなら入れなくてよい。

1. インストールする(wingetの場合)。

   ```powershell
   winget install --id UB-Mannheim.TesseractOCR -e
   ```

   wingetで入らないときは、[UB Mannheimの配布ページ](https://github.com/UB-Mannheim/tesseract/wiki)のインストーラを使ってもよい。

2. **PATHは通さなくてよい。** Chihayaburuは、PATH上の`tesseract`と、次の既定のインストール先を自動で探す。

   ```
   C:\Program Files\Tesseract-OCR
   C:\Program Files (x86)\Tesseract-OCR
   %LOCALAPPDATA%\Programs\Tesseract-OCR
   ```

   これ以外の場所に入れたときは、環境変数`PDF_VIEW_TESSERACT`に`tesseract.exe`のフルパスを指定する(例: `D:\tools\Tesseract-OCR\tesseract.exe`)。

3. **日本語の言語データを入れる。** インストールしただけだと、日本語(`jpn`)が入っていないことがある。
   [jpn.traineddata](https://github.com/tesseract-ocr/tessdata_fast/raw/main/jpn.traineddata)をダウンロードして、`C:\Program Files\Tesseract-OCR\tessdata\`に置く(管理者権限が要る)。
   インストーラを使うなら、言語の選択の画面で「Japanese」にチェックを入れてもよい。

4. 確認する。`eng`と`jpn`が表示されればよい(PATHを通していないときは、フルパスで実行する)。

   ```powershell
   & "C:\Program Files\Tesseract-OCR\tesseract.exe" --list-langs
   ```

困ったとき:
- **「tesseractコマンドが見つかりません」と出る**: 上の既定の場所にもPATHにも無い。`PDF_VIEW_TESSERACT`でフルパスを指定するか、既定の場所に入れ直す。環境変数を変えたあとは、Chihayaburuも起動し直す。
- **日本語が読めない**: 手順3の`jpn`が入っていない。`tesseract --list-langs`で確認する。
- **別の言語を使いたい**: 環境変数`PDF_VIEW_OCR_LANG`にTesseractの書式で指定する(例: `jpn+eng`)。

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
