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

## ③ pdf_viewer (1ページ表示)
- gpui は `rev = fae8a35...` 固定で依存に追加。初回の `cargo check` は約4.5分。リンクまで通る (`cargo test -p pdf_viewer` 成功) ので、システムライブラリは足りている。
- GPUIの `RenderImage` は **BGRA** (中身は `image::Frame` / `RgbaImage`) なので、`to_render_image` でR/Bを入れ替えている。`image` crate (default-features off) を直接依存に追加。
- `RenderImage::scale_factor` は `pub(crate)` で変更できない。→ 画像はウィンドウのDPI(`window.scale_factor()`) × ズームの解像度で描画し、`img().w().h()` で論理サイズを明示して表示する。HiDPIでもぼやけない。
- `RenderImage` は生成のたびに新しい `ImageId` を持ち、GPUにアップロードされる。毎フレーム作り直さないよう `PdfView` が (ページ, 倍率) をキーに保持し、変わった時だけ作り直す。古い画像は `window.drop_image` でアトラスから破棄する。
- Zed組み込みを見据え、`PdfView::new(Arc<PdfDocument>, cx)` はファイルI/Oを持たない。`image_viewer` の `Item` 実装は、組み込み時に別途ラップする想定。
- 描画エラー(範囲外など)はビュー内にテキスト表示する。
- 変換関数のテスト3件。`Render` は目視確認が必要なので ④ で確認する。

## ④ pdf_view_app (ウィンドウ表示)
- 方針: Zed本体への組み込み(Item化)は、単体アプリ(A)が完成してから着手する(B)。Zed拡張機能(WASM)には独自ペイン/Itemを追加するAPIがないため、拡張機能としては配布できない。
- 起動API: このrevでは `gpui_platform::application().run(...)` を使う (`gpui::App::new` ではない)。`gpui_platform` を `default-features = false, features = ["wayland", "x11"]` で追加。ネイティブLinuxではシステムフォントが使われるのでフォント登録は不要。
- 窓を開く際、`cx.new` を使うには `gpui::AppContext as _` の import が必要。
- 引数エラー・読み込みエラーは標準エラーに出して終了コード1。`pdf_path_from_args` は単体テスト3件。
- 動作確認:
  - 引数なし / 存在しないファイルは想定通りのメッセージで終了コード1。
  - `two_pages.pdf` で起動し、8秒間パニックなく動作した。EGL/MESAの警告はWSL2のGPUドライバ由来で、ソフトウェア描画にフォールバックしている。
  - WSLgではウィンドウのスクリーンショット (`import -window root`) が取得できなかったため、見た目はユーザーの目視確認に委ねた。

## リファクタリング (①〜④後)
- 目視確認: `two_pages.pdf` の1ページ目が表示された。赤い矩形が左下に出るのは正しい (PDFは左下原点、矩形は (100,100)-(300,300))。
- 挙動を変えない整理のみ行った: `document.rs` のimport統合、`validate_scale` の切り出し、`RenderedPage::from_pixmap` への変換移動。テスト19件・clippy警告なし。
