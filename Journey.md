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

## ⑤ スクロール + ズーム
- 設計: 計算とGPUI依存部を分離した。
  - `layout.rs` (全ページ縦並びの座標・可視範囲・現在ページ・ページ先頭位置) と `zoom.rs` (倍率の制限・ホイール係数・ズーム時のスクロール補正) は純粋関数で、単体テスト13件。
  - `images.rs` (`PageImages`) は表示範囲内のページ画像だけを保持し、範囲外は `window.drop_image` で破棄する。
  - `view.rs` (`PdfView`) と `actions.rs` が組み立てる。
- スクロール: GPUIの `overflow_scroll` + `ScrollHandle` を使い、中身は全体サイズ固定の `relative` な div に、見えているページだけを `absolute` で置く (見えないページは何も描画しない)。`uniform_list` はページごとに高さが違うため使えない。
- GPUIのAPIで分かったこと:
  - このrevでは `ScrollWheelEvent` が `ScrollEvent` に、`on_scroll_wheel` が `on_scroll` に改名されている (旧名はdeprecated)。
  - `overflow_scroll` の標準のスクロール処理は、ユーザーの `on_scroll` とは別に動く。Ctrl+ホイールでは両方が動いてしまうので、ズーム側は新しい位置を `pending_offset` に入れ、次の `render` の冒頭で `set_offset` して上書きする。
  - `ScrollHandle::bounds()` は初回描画後でないと0。初回だけ `window.viewport_size()` で代用する。
- 操作: Ctrl/Cmd+ホイール、Ctrl+`+`/`-`/`0` でズーム (0.25〜5.0、ステップ1.25倍、ビューポート中央または指の位置を保つ)。PageUp/PageDown/Home/End でページ移動。キーバインドは `pdf_viewer::init(cx)` で登録する (Zed組み込み時はkeymap jsonで定義する想定)。下部の帯に「現在のページ / 全ページ・倍率」を表示。
- 既知の制約:
  - 描画はメインスレッドで同期的に行う。大きいページを高倍率で見るとホイール操作が重くなる可能性がある。バックグラウンド描画 (`cx.background_spawn`) は今後の課題。
  - ホイールズームごとに倍率が連続的に変わり、そのつどキャッシュに新しいキーが入る。LRUは16枚なので古いものから消える。
- `examples/zed/tasks.json` に、現在のファイルを `pdf_view_app $ZED_FILE` で開くタスク例を追加。

## ⑥ 追加機能 (矢印キー・ツールバー・自前タイトルバー)
⑤の目視確認後に、ユーザーから3つの追加要望があった。仕様に幅があったため、ツールバーの内容(ページ番号入力+総ページ数、ズーム操作)とウィンドウ操作の方式(自前タイトルバー)を確認してから実装した。

### 1. 左右矢印キーでページ移動
- `left`/`right` を `PreviousPage`/`NextPage` に割り当てた。拡大中の横スクロールは矢印キーではできなくなる (ホイール/Shift+ホイールを使う)。

### 2. ブラウザ風ツールバー (`pdf_viewer`)
- 構成: `‹ [ 3 ] / 40 ›`、`− 100% +`、`Width` / `Page`。下部の情報帯は廃止してツールバーに集約。
- ツールバーは `PdfView` の一部にした。Zed組み込み後もそのまま使える。
- フィット表示: `ZoomMode { Manual, FitWidth, FitPage }`。`Fit*` の間は毎フレーム `apply_fit` でビューポートから倍率を計算し直す (リサイズに追従)。手動でズームすると `Manual` に戻る。倍率計算 (`fit_width`/`fit_page`) は純粋関数でテスト済み。
- ページ番号入力:
  - GPUIには標準の入力欄がないので、フォーカス可能な `div` + `on_key_down` で最小限のものを自作した。編集ロジックは `PageInput` (数字のみ・最大6桁・Enterで1〜総ページ数に丸めて確定) に分離してテスト済み。
  - 編集中はルートの `key_context` を外す。そうしないと、`left`/`home`/`end` などが入力欄でなくページ移動のアクションに奪われる。
  - 入力欄からフォーカスが外れたら (別の場所のクリックなど)、`render` で編集を取り消す。ページ領域のクリックでビュー本体にフォーカスを戻す。
- ラベルは、システムフォントに日本語が無い環境での豆腐を避けるため ASCII (`Width`/`Page`) にした。
- Rust 2024 の罠: `&mut Context` を受けて `impl IntoElement` を返す関数は、戻り値が `cx` の借用を捕捉してしまい、呼び出し後に `cx.listener` が使えなくなる。`impl IntoElement + use<>` で捕捉を外す。
- テスト: 純粋関数 (zoom/layout/page_input) 24件。

### 3. 自前タイトルバー (`pdf_view_app` のみ)
- `WindowOptions::window_decorations = Client` を要求し、`AppWindow` (タイトルバー + PdfView + リサイズ領域) で包む。Zed組み込み用の `pdf_viewer` には影響しない。
- OS側が枠を描く環境 (`window.window_decorations()` が `Server`) では、二重にならないよう自前のタイトルバーとリサイズ領域は出さない。
- タイトルバー: 右上に最小化/最大化(復元)/閉じるボタン。空き部分のドラッグでウィンドウ移動、ダブルクリックで最大化/復元。
  - ドラッグは、マウス押下で `should_move` を立て、最初の `mouse_move` で `start_window_move` する (押下直後に呼ぶとダブルクリックが取れなくなるため)。ボタン上の押下は `stop_propagation` でドラッグ開始を防ぐ。
  - アイコンは、フォント依存を避けるため `div` の枠線で描いた (閉じるだけは `×`)。Windows向けに `window_control_area` も指定してある。
- 縁・角 8 方向の透明な `absolute` 領域で `start_window_resize`。最大化中は出さない。影や丸角は付けていない。
- 未確認: WSLgでクライアント装飾が期待どおり動くか (ボタン・ドラッグ・リサイズ) は、このセッションからは目視できていない。起動とクラッシュしないことのみ確認。

## B. Zed本体への組み込み: 調査 (rev fae8a35)
`image_viewer` を参考に、PDFをタブで開くために必要なものを調べた。

- 拡張機能(WASM)からは実現できない。Zed本体に crate を追加する形になる。
- ファイルを開く仕組み:
  - `workspace::register_project_item::<V>(cx)` で登録する。`V: workspace::item::ProjectItem`、`V::Item: project::ProjectItem`。
  - `project::ProjectItem::try_open` が `Some(Task)` を返したビューが、そのファイルを開く。`None` なら次の候補へ。
  - レジストリは**後から登録したものを先に試す** (`.iter().rev()`)。`main.rs` で `editor::init` の後に登録すれば、PDFをエディタより先に判定できる。
  - `project::ProjectItem` は `project` crate の外でも実装できる。→ `ImageItem` と同様の `PdfItem` を、組み込み用の crate 側に作れば `project` crate の改造は不要。
- `workspace::item::Item` の必須メソッドは `tab_content_text` だけ。他は既定実装がある (`Focusable + EventEmitter + Render` は必要)。`can_split`/`clone_on_split`、`tab_icon`、`tab_tooltip_text`、`for_each_project_item` などを必要に応じて実装する。
- ファイル読み込み: `Worktree::load_binary_file(path, cx)` が `LoadedBinaryFile { file, content: Vec<u8> }` を返す。ローカルworktreeのみ対応 (リモートは「未対応」エラー)。→ `PdfDocument::from_bytes` が必要。
- 組み込み先は `crates/zed/src/main.rs` の `image_viewer::init(cx)` の隣に1行足すだけ。他に、ルートの `Cargo.toml` (members と workspace.dependencies) と `crates/zed/Cargo.toml` の変更が必要。
- `image_viewer` は GPL-3.0-or-later。Zed本体もGPL。本リポジトリの crate は Apache-2.0 (GPLv3組み込みは可能)。`image_viewer` のコードは本リポジトリの crate にコピーしない。Zed側に置く組み込み用 crate は GPL-3.0-or-later にする。
- 想定される罠:
  - gpui が二重になる: 本リポジトリの `gpui` はgit依存、Zed内では `path` 依存。型が食い違うので、Zed側の `Cargo.toml` に `[patch."https://github.com/zed-industries/zed"] gpui = { path = "crates/gpui" }` が必要。
  - `pdf_viewer` の色は固定値。Zedに入れるとテーマに合わせたい → 色を `PdfViewStyle` として外から渡せるようにする。
  - Zed の workspace lint、`LICENSE-GPL`、依存ライセンスチェック (`script/licenses/zed-licenses.toml`) に新 crate / 新依存 (hayro 等) が通るか。
- 環境: 24コア / RAM 14GB / ディスク空き 881GB。Zed本体のフルビルドは初回に長時間かかり、ディスクも数十GB使う見込み。リンク時のメモリが逼迫する可能性がある。
