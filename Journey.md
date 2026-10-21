# 開発の軌跡 (Journey)

Zed向けGPUI製PDFビューアの開発過程を記録する。

## 全体方針
- Cargoワークスペースで3crate構成: `pdf_core` / `pdf_viewer` / `pdf_view_app`
- 手順: ①雛形 → ②pdf_core → ③pdf_viewer(1ページ) → ④アプリ表示 → ⑤スクロール+ズーム
- 着手前に方針確認、1関数ごとにコミット、3〜5コミットでリファクタ、pushは確認後

## ① ワークスペース雛形
- 方針を提示し承認を得た。
- Rust toolchain: Zed本体の `rust-toolchain.toml` (rev fae8a35 時点で 1.98.1) に合わせて固定した。手元の既定は 1.95.0 だったが、rustup が 1.98.1 を取得した。
- edition は 2024。共通設定は `[workspace.package]` に集約。
- GPUIのrev候補: `fae8a354829e56bf5615fedfa8cdfecb124550c4` (zed main HEAD, 2026-10-21 取得)。pdf_viewer の依存追加時(③)に固定する。
- 各crateは空の雛形で `cargo check` が通ることを確認。

## ② pdf_core
- hayro 0.8 を採用 (`hayro::hayro_syntax` / `hayro::vello_cpu` が再エクスポートされるので直接依存は `hayro` のみ)。
- API確認で分かったこと:
  - `Pdf::new(Arc<Vec<u8>>)` はデータを所有する。`Pdf::pages()` / `hayro::render()` は `Pdf` を借用する。
  - `RenderCache<'a>` は `Pdf` を借用するため、構造体に持たせると自己参照になる。→ 呼び出しごとに生成し、ページ画像のキャッシュは自前のLRUで持つことにした。
  - `Pixmap` は premultiplied RGBA8。背景を不透明な白にすれば straight RGBA と同じ値になるので変換不要。
  - 画像サイズは `u16` なので、`幅×倍率` が 65535 を超える倍率は `InvalidScale` で弾く。
  - `InterpreterSettings` は `hayro::hayro_interpret` 経由でしか参照できない (`hayro` 直下では private)。
- 設計:
  - `render_page(&self, index, scale) -> Arc<RenderedPage>`。内部に `Mutex<LruCache<(index, scale.to_bits()), Arc<..>>>` を持ち、`&self` で呼べる。`PdfDocument` は `Send + Sync` (テストで確認) なので、後からバックグラウンド描画にも使える。
  - 描画中はロックを外す。同じページを同時に描画した場合は重複計算が起こりうるが許容する。
  - キャッシュは枚数ベース (既定16枚、`set_cache_capacity` で変更)。メモリ量ベースへの変更は必要になってから検討する。
- テスト用PDF (`tests/fixtures/two_pages.pdf`) は手書き生成。A4と300x200の2ページで、フォント不要。
- テスト13件、clippy警告なし。
