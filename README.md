# zed_pdf-view
Zed用のpdf表示の拡張機能

## ビルド

### 必要なもの
- [rustup](https://rustup.rs/)(`rust-toolchain.toml` により Rust 1.98.1 が自動で取得される)
- Cコンパイラとリンカ
  - Linux: `gcc` などのビルドツール一式
  - Windows (MSVC): [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/) の「C++ によるデスクトップ開発」。VS Code だけでは足りない(`link.exe` が必要)

初回は依存crate(gpui など)のビルドに数分かかる。

### OCR(任意)
画像だけで作られたPDFの文字を選択・コピーするには、ツールバーの`OCR`ボタンから「This page」(表示中のページ)か「All pages」(全ページ)を選んでOCRする。実行中は`Stop`で中断できる。ショートカットは Ctrl+Shift+R(表示中のページ)と Ctrl+Alt+R(全ページ)。
外部コマンドの [Tesseract](https://github.com/tesseract-ocr/tesseract) を使うので、インストールして、言語データ(日本語は `jpn`、英語は `eng`)を用意しておく。
`tesseract` はPATHから探す(Windowsでは、PATHになくても既定のインストール先を探す)。別の場所のものを使うときは、環境変数 `PDF_VIEW_TESSERACT` にそのパスを指定する。
結果はメモリ上にだけ持ち、PDFファイルは変更しない。言語は環境変数 `PDF_VIEW_OCR_LANG`(例: `jpn+eng`)で上書きできる。

### ビルドと実行
```sh
# ビルド
cargo build --release

# PDFを指定して起動(省略するとPDFを開かずに起動する)
cargo run --release -p pdf_view_app -- path/to/file.pdf
```

ビルドした実行ファイルは `target/release/pdf_view_app`。

### 自動の再読み込み
表示中のPDFが書き換わると、約1秒以内に自動で読み直す(スクロール位置・ズーム・回転は保つ)。
TypstやLaTeXでPDFを書き出し続けるときの、プレビューとして使える。書き込みの途中や、読めないファイルのときは、古い表示を残して待つ。
読み直すと、OCRの結果は消える(もう一度OCRする)。

### テスト
```sh
cargo test
```

## 依存関係

### ツールチェーン
- Rust 1.98.1 (edition 2024)。`rust-toolchain.toml` で固定

### 依存crate
バージョンは `Cargo.toml` の `[workspace.dependencies]` で一括管理している。

| crate | バージョン | 用途 | 使うcrate |
|---|---|---|---|
| [hayro](https://crates.io/crates/hayro) | 0.8 | PDFのパースとラスタライズ | pdf_core |
| [thiserror](https://crates.io/crates/thiserror) | 2 | エラー型の定義 | pdf_core |
| [memmap2](https://crates.io/crates/memmap2) | 0.9 | PDFファイルのメモリマップ読み込み | pdf_core |
| [gpui](https://github.com/zed-industries/zed) | git (rev `fae8a35`) | UIフレームワーク | pdf_viewer, pdf_view_app |
| gpui_platform | git (gpuiと同じrev) | プラットフォーム層 (Linuxは `wayland` / `x11`) | pdf_view_app |
| [image](https://crates.io/crates/image) | 0.25 (デフォルト機能なし) | 画像データの扱い | pdf_viewer |
| [log](https://crates.io/crates/log) | 0.4 | ログ出力 | pdf_viewer, pdf_view_app |

gpui と gpui_platform は Zed リポジトリの特定コミットを git から取得する。

### ワークスペース内のcrate
- `pdf_core`: PDFの読み込み・描画 (hayro に依存)
- `pdf_viewer`: ビューア部分 (pdf_core, gpui に依存)
- `pdf_view_app`: 実行ファイル (pdf_core, pdf_viewer, gpui に依存)

### テスト用
- `gpui` の `test-support` 機能 (pdf_viewer, pdf_view_app の dev-dependencies)
