# zed_pdf-view
Zed用のpdf表示の拡張機能

## ビルド

### 必要なもの
- [rustup](https://rustup.rs/)(`rust-toolchain.toml` により Rust 1.98.1 が自動で取得される)
- Cコンパイラとリンカ
  - Linux: `gcc` などのビルドツール一式
  - Windows (MSVC): [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/) の「C++ によるデスクトップ開発」。VS Code だけでは足りない(`link.exe` が必要)

初回は依存crate(gpui など)のビルドに数分かかる。

### ビルドと実行
```sh
# ビルド
cargo build --release

# PDFを指定して起動(省略するとPDFを開かずに起動する)
cargo run --release -p pdf_view_app -- path/to/file.pdf
```

ビルドした実行ファイルは `target/release/pdf_view_app`。

### テスト
```sh
cargo test
```
