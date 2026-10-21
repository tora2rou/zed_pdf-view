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

## 方針転換: 単体アプリを先に充実させる
- Zed組み込み(B)は後回しにした。調査結果は上に残してある。
- 優先機能をユーザーに確認し、次の順で進めることにした。
  1. ドラッグスクロール・回転・全画面・ダークモード (小)
  2. ファイルを開く操作 (Ctrl+O、ドラッグ&ドロップ、最近開いたファイル)
  3. 描画の高速化 (バックグラウンド描画・先読み)
  4. サイドバー (サムネイル・目次)
  5. リンクのクリック (ページ内・URL)
  6. テキスト選択とコピー (hayroから文字位置を取れるか先に調査)
- 後ろの段階は前の基盤を使う。サムネイルは3の描画基盤を使い、選択モードは手のひらツール(1)と切り替える。

## 段階1: 表示まわりの操作
- 回転 (`R` / `Shift+R`、`Rotate`ボタン): 画像のピクセルを回す純粋関数 (`transform.rs`) で実装。回転でページの縦横が入れ替わるので、レイアウト用の`page_sizes`(回転後)と元の`base_sizes`を分けて持つ。回転後は、見ていたページの先頭に合わせ直す。PDFファイルは変更しない。
- ダークモード (`D` / `Dark`ボタン): RGBを反転する (アルファは保つ)。画像や色付きの図も反転する。色相を戻す調整は今後の課題。
- 回転と反転は `Appearance` にまとめ、画像キャッシュの条件 (`ImageKey` = 倍率 + `Appearance`) に含めた。
- 全画面 (`F11` / `Full`ボタン): `window.toggle_fullscreen()`。全画面中は自前タイトルバーとリサイズ領域を隠す (`is_fullscreen`)。
- ドラッグスクロール: ページ領域で左ボタンを押して動かす。移動量の計算は純粋関数 `dragged_scroll` (範囲内に収める)。ボタンを離したのを取り逃した場合は、次のマウス移動で`pressed_button`を見て終了する。カーソルは開いた手/つかんだ手。
- リファクタ: `view.rs` が500行を超えたので、ページ番号入力のメソッドをツールバーの機能として `toolbar.rs` に移した。
- テスト: 純粋関数 (zoom/layout/page_input/transform/pan) で計 36 件。
- 未確認: ドラッグの操作感、`D`/`R`/`F11` の動作、全画面中の見た目は目視確認待ち。

## 段階2: ファイルを開く操作 (`pdf_view_app`)
方針の細部は、ユーザーに確認せず次の既定で実装した (違えば直す): 開くときは同じウィンドウの文書を入れ替える / 引数なしで起動したら空のウィンドウに案内と最近のファイルを出す / 最近のファイルは設定ディレクトリのテキストファイルに保存する。

- `PdfView::set_document`: 文書を入れ替える。GPU上の画像を全部破棄 (`PageImages::clear`) し、先頭に戻る。ズーム・回転・反転などの表示設定は引き継ぐ。
- `AppWindow` が `Option<Entity<PdfView>>` を持つ。PDFを開くまではビューが無く、案内を表示する。
- 読み込みは `cx.background_spawn` で行い、終わったらUIスレッドで差し替える (`spawn_in` + `update_in`)。大きなPDFでもUIが固まらない。失敗したらウィンドウ上部の赤い帯に理由を出す (成功すると消える)。
- 開く方法:
  - `Ctrl+O` / `Cmd+O` (`cx.prompt_for_paths`)。キャンセルやダイアログを出せなかった場合は何もしない。
  - ウィンドウへのドロップ (`ExternalPaths`)。複数ならば先頭を開く。
  - タイトルバーの `Open` ボタン。
  - 最近のファイル: タイトルバーの `Recent` ボタンか `Ctrl+Shift+O` でオーバーレイ表示。外側のクリックで閉じる。引数なし起動時の案内にも同じ一覧を出す。サーバーサイド装飾の環境ではタイトルバーのボタンが出ないが、ショートカットとドロップは使える。
- 最近のファイル (`recent.rs`): 新しい順、重複は先頭へ移す、最大10件、1行1パスのテキスト。保存先は `$XDG_CONFIG_HOME/pdf_view_app/recent.txt` (なければ `~/.config/...`、Windowsは `%APPDATA%`)。保存に失敗しても動作には影響させず、標準エラーに出すだけ。追加の依存 (serde等) は使っていない。
- 起動時の引数: 引数なしは正常 (案内を表示)、2つ以上は使い方を出して終了コード1。引数のファイルが開けないときは従来どおりウィンドウを出さずに終了コード1 (スクリプトから使いやすい)。
- キーバインドはアプリ側 (`app_window::init`) で `PdfViewApp` コンテキストに登録。`PdfView` の編集中は `PdfViewer` コンテキストを外しているが、`PdfViewApp` は残るので `Ctrl+O` はページ番号入力中も効く。
- リファクタ: タイトルバーのボタンで重複していた「ドラッグ開始を防ぐ」処理を `block_drag` にまとめた。
- テスト: `recent.rs` 7件、引数処理3件。ダイアログやドロップは目視確認待ち。起動 (引数なし / ファイルあり / 存在しないファイル / 引数2つ) は確認済み。

## 段階2の不具合修正 (ユーザー報告: ダイアログが出ない / Recentから開けない)
### Recentの項目が開けない
- 原因: 空の状態では、案内内の一覧とオーバーレイの一覧に、同じ要素ID (`("recent", index)`) の項目が二重に存在した。GPUIは要素IDで「押下中」の状態を共有する。背面側の項目 (ホバーされていない) が mouse up の capture フェーズで押下状態を先に消費してしまい、前面の項目の `on_click` が呼ばれなかった。
- 切り分けの手順 (GPUIの `test-support` を dev-dependency に追加して、`TestAppContext` の `simulate_click` で再現した):
  1. バックドロップのクリックで閉じることを確認 (テストの仕掛けは正しい)。
  2. 項目の `mouse_down` / `mouse_up` は届くが `on_click` だけ呼ばれないことを確認。
  3. 単純な要素の `on_click` はテスト内で動くことを確認 (GPUIの問題ではない)。
  4. `occlude` / `hover` / `overflow_hidden` / `on_drop` / `key_context` を一つずつ外しても直らない → 要素IDの重複に気づいた。
- 修正: 一覧の描画に `id_prefix` を渡し、案内用 (`recent-inline`) とオーバーレイ用 (`recent-overlay`) で別のIDにした。回帰テストを2件追加 (オーバーレイの項目 / 案内内の項目)。
- 教訓: 同時に表示されうる同じ見た目の部品には、必ず別のIDを付ける。
- 途中でパネルを兄弟構造 + `occlude` に直したが、これは原因ではなかった。ただしパネルの `stop_propagation` で背面を止める方式より素直なので、そのまま残した。

### ファイル選択ダイアログが出ない
- 原因: この環境 (WSL2) には xdg-desktop-portal が無い。gpuiのLinux実装は、ファイル選択にポータル (ashpd) を使い、無いと `Couldn't open file picker due to missing xdg-desktop-portal implementation.` のエラーを返す。アプリはそのエラーを無視していたので、何も起きないように見えた。`zenity` / `kdialog` も無い。
- 対応 (第一段階): エラーを画面上部の赤い帯に出す (「ドロップするか Recent を使う」案内つき)。
- 未解決: ダイアログ自体を出す方法。選択肢は (A) 環境にポータルを入れる (`xdg-desktop-portal` + `xdg-desktop-portal-gtk` など。sudoが必要) / (B) アプリ内蔵のファイルブラウザを作る / (C) パスを直接入力する簡易入力。ユーザーの判断待ち。

### Ctrl+Oが効かない (ユーザー報告: ポータルを入れると`Open`ボタンは動くが`Ctrl+O`は動かない)
- 再現: テスト (`simulate_keystrokes("ctrl-o")` + `did_prompt_for_paths`) で、PDFを開く前 (空の状態) だけ失敗した。PDFを開いた後は動いていた。
- 原因: GPUIのキーバインドは「フォーカスのある要素から根に向かう要素のコンテキスト」で解決される。空の状態ではフォーカスを持つ要素が無く、`AppWindow` の `key_context("PdfViewApp")` が経路に入らないため、アクションがどこにも届かなかった。ボタンは`on_click`で直接メソッドを呼ぶので影響を受けなかった。
- 修正: `AppWindow` 自身が `FocusHandle` を持ち、`track_focus` する。起動時にPDFが無ければここにフォーカスを置く。PDFを開いたらビューにフォーカスを移す (従来どおり)。
- テスト: 空の状態 / 閲覧中の `Ctrl+O`、`Ctrl+Shift+O` の開閉 (計3件) を追加。
- 教訓: キーボードショートカットを持つビューは、ボタンだけで確認せず、フォーカスが無い状態でもテストする。

## 段階3: 描画の高速化
- 計測: ユーザーの実PDF (`sample.pdf`, 33ページ・約1MB) をreleaseで描画すると、1ページ13〜32ms (倍率1で平均16ms、倍率2で平均27ms)。1フレーム16msの予算を超えるので、新しいページが見えるたびに画面が止まっていた。テスト用の図形だけのPDFでは3〜9msで、問題が見えにくかった。
- 方針: 描画 (ラスタライズ + 回転/反転 + BGRA変換) を `cx.background_spawn` に移し、UIスレッドでは画像の受け取りと表示だけにする。
- 設計 (`images.rs`、GPUIの`Window`に依存しない状態管理):
  - `lookup(index, key)` は `Fresh` (条件が合う画像) / `Stale` (倍率などが古い画像) / `Failed` / `Missing` を返す。`Stale` は新しいページの大きさに引き伸ばして仮表示する。回転が違う画像は縦横比が崩れるので仮表示に使わない (反転だけの違いは使う)。
  - `request` で要求を登録し、`complete` で結果を受け取る。登録した最新の `ImageKey` と違う結果 (ズームが変わった、スクロールで範囲外に出た) は捨てる。
  - `ImageKey` に世代 (`generation`) を含める。文書を入れ替える (`clear`) と世代が進み、古い文書の描画結果が新しい文書に混ざらない。
  - 置き換わった画像・捨てた画像は戻り値で返し、呼び出し側が `window.drop_image` でGPUから破棄する。
- 先読み: 見えているページの前後1ページを描画しておく。画面外に出たページの画像と要求は破棄する。
- ズーム直後の延期: 最後のズーム操作から120ms以内は、仮表示できる古い画像があるページの再描画を要求しない。連続したズーム (ホイール) の途中で、すぐ古くなる描画をしないため。落ち着く頃にタイマーで再描画する。画像が何も無いページは待たずに描画する。
- 描画までの間は、ページの大きさの白い枠 (ダークモード中は暗い枠) を出す。
- 時計: 延期の判定には `std::time::Instant` でなく `cx.background_executor().now()` を使う。テストの偽の時計で進められる (最初は実時間を使っていて、テストが失敗した)。
- テスト: `images.rs` の状態遷移12件 (純粋)、GPUIのテスト3件 (バックグラウンドで描画される / 文書を入れ替えると新しい文書が描画される / ズームで仮表示→落ち着いたら高解像度で描き直す)。
- 残る主スレッドの負荷: 新しい画像のGPUへの転送 (描画時)。1190x1683の画像で約8MB。今のところ測っていない。
- 既知の制約: `PdfDocument` のLRU (16枚) と、`PageImages` が持つ画像 (見えている範囲 + 先読み) で、同じページを二重に持つ。メモリが気になる場合はLRUの枚数を減らす。
- 未確認: 実際の体感は目視確認待ち (`sample.pdf` で起動して8秒間パニックなく動作したことのみ確認)。

## 段階4: サイドバー (サムネイルと目次)
方針の細部は確認せず次の既定で実装した (違えば直す): 既定は非表示 / ツールバーの `Side` ボタンか `F9` で開閉 / タブは「Pages」(サムネイル)と「Outline」(目次) / 幅は固定 (220px)。

### pdf_core
- `render_page_uncached`: 文書のLRUキャッシュに載せない描画。サムネイルは大量に作るので、本文用の16枚を押し出さないために別にした。
- `PdfDocument::outline() -> &[OutlineItem]` (`OutlineItem { title, page: Option<usize>, children }`)。初回だけ読み込む (`OnceLock`)。
  - hayroの低レベルAPI (`hayro::hayro_syntax::object`) で、カタログ → `/Outlines` → `/First` / `/Next` をたどる。ページの対応づけは、各ページ辞書の `obj_id()` を表にして、宛先の配列の先頭の参照と突き合わせる。
  - 宛先の4形式を読む: `/Dest` の配列 / `/A` の GoTo アクションの `/D` / 名前付き宛先 (カタログの `/Dests` 辞書) / 名前付き宛先 (`/Names` → `/Dests` のネームツリー。`/Kids` を再帰)。
  - 題名は PDF の「テキスト文字列」: UTF-16BE (BOM付き) / UTF-8 (BOM付き) / それ以外は Latin-1 で近似 (PDFDocEncoding との差は一部の記号のみ)。
  - 安全弁: 項目数 5万 / 階層 32 / ネームツリー 16 / 訪問済みの項目で循環検出。循環する壊れたPDFでも終わることをテストした。
  - 実PDF (`sample.pdf`) の日本語・5階層の目次が正しく読めた。
- テスト用に `with_outline.pdf` を生成 (上の4形式 + UTF-16の題名)。

### pdf_viewer
- `outline_rows.rs`: 目次を「開いている項目の子だけ」の行リストにする純粋関数と、開閉の切り替え。
- `sidebar.rs`:
  - サムネイル: セルの幅は固定、高さはページの縦横比 + ページ番号の行。本文と同じ流儀 (`PageImages`, `layout`) で、見えている範囲と前後1枚をバックグラウンドで描画する。本文とは別の `PageImages` を持つ。現在のページは枠で強調し、本文のページが変わったら一覧もそこまでスクロールする。
  - 目次: 行をクリックでそのページへ。左の `+` / `−` で子を開閉する。行の中の「開閉」と「移動」は別の要素にした (親子で `on_click` を両方持つと、クリックで両方発火するため)。リンク先が読めない項目は薄く表示して無効にする。
- `PdfView` の描画要求を `RenderRequest` (index/key/scale/target/mode) にまとめ、本文とサムネイルの両方に使う。
- ページ領域の大きさが前回と変わったら、もう一度描画する (`last_viewport`)。サイドバーの開閉やウィンドウのリサイズのとき、`Width`/`Page` のフィット表示が1フレーム遅れて追従しないのを防ぐ。
- リファクタ: 148行あった `render` を、描画要求 (`request_page_images`)・ページの組み立て (`render_pages`)・操作領域 (`render_pages_area`) に分け、68行にした。ついでに、整形で外れて残っていた不要なクラス (`w_full` など) を直した。
- テスト: GPUIのテストで、サイドバーを開くとサムネイルが描画される / 閉じている間は描画しない / 目次の行・サムネイルのクリックで該当ページへスクロールする / `+` で子が開く。実PDFでは、サムネイルは見えている分と先読み分の6/33ページだけ描画され、目次の最上位は9行だった。
- テストの落とし穴: 目次クリックの検証で、テスト用のページが小さくて全体が画面に収まり、スクロール量が0のままになった (コードの不具合ではない)。ズームを上げてスクロール可能にして検証した。

### 確認待ち
- 実際の見た目と操作感 (サイドバーの開閉、サムネイルの表示速度、目次の開閉、クリック移動) は目視確認待ち。
