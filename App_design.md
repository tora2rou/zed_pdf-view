# Chihayaburu・Tuzuri の設計

このリポジトリには、2つのアプリがある。

- **Chihayaburu**: GPUI製のPDFビューア。単体のアプリ (`pdf_view_app`) として動き、表示部分はライブラリ (`pdf_viewer`) として他のアプリにも組み込める。Typst (`.typ`) を開くと、コンパイルしたPDFを表示する。
- **Tuzuri**: Zed (コードエディタ) のforkに、Chihayaburuを組み込んだ組版用のエディタ (ブランチ `Tuzuri`)。TypstをZedで編集しながら、アプリの中でプレビュー・エラー表示・PDFへの書き出しができる。

TuzuriはChihayaburuの部品をそのまま使う。Chihayaburuの部品はZedに依存しないので、単体のアプリとTuzuriの両方で使える。

設計の目的 (`design.md`) で最も大きいのは、**少ないメモリで動くこと**。

## 全体の構成

```
            ┌───────────────────────────── Tuzuri (Zedのfork) ─────────────────────────────┐
            │  zed (実行ファイル tuzuri)                                                    │
            │    ├─ typst_ui   … Typstのプレビュー・デバッグパネル・エディタの波線・書き出し  │
            │    ├─ pdf_tab    … .pdf をZedのタブで開く                                     │
            │    ├─ grammars/typst … Typstの色分け (tree-sitter)                            │
            │    └─ Zedの部品 (workspace, project, editor, language, lsp, ui, …)              │
            └──────────────┬───────────────────────────────┬──────────────────────────────┘
                           │                               │
 ┌──────── Chihayaburu ────┼───────────────────────────────┼────────────┐
 │  pdf_view_app (単体のアプリ)                              │            │
 │      │                  │                               │            │
 │      ▼                  ▼                               ▼            │
 │  pdf_viewer (GPUIの表示部品: PdfView)              typst_build        │
 │      │                                          (Typst → PDF。GUI非依存)│
 │      ▼                                                                │
 │  pdf_core (PDFの読み込み・描画・文字・リンク・目次・OCR。GUI非依存)       │
 └──────────────────────────────────────────────────────────────────────┘
```

| crate | 役割 | 依存するUI | 規模 |
|---|---|---|---|
| `pdf_core` | PDFの読み込み・ラスタライズ・文字の取り出し・リンク・目次・OCR | なし | 約2,100行 |
| `pdf_viewer` | PDFを表示するビュー (`PdfView`)。スクロール・ズーム・選択・サイドバー・ツールバー | GPUI | 約5,400行 |
| `pdf_view_app` | Chihayaburuの単体アプリ (ウィンドウ・タブ・タイトルバー・最近のファイル・自動再読み込み・Typstのタブ) | GPUI | 約2,600行 |
| `typst_build` | Typstのコンパイル、編集中の内容でのコンパイル (Session)、エラーの位置、書き出し (`tuzuri export`) | なし | 約1,100行 |
| `pdf_tab` | Tuzuriで `.pdf` をタブとして開く | Zed | 約190行 |
| `typst_ui` | TuzuriのTypst機能 (共有の文書・プレビュー・デバッグパネル・波線・書き出し) | Zed | 約1,700行 |

**層の分け方**: GUIに依存しない処理 (`pdf_core`、`typst_build`) と、計算だけの部分 (`pdf_viewer` の `layout.rs`・`zoom.rs`・`selection.rs` など) を、GPUIの部品から分けている。そのため、単体のテストで多くを確かめられ、別のアプリにも組み込みやすい。

## 使用した技術

### 言語・ツール
| 技術 | 用途 |
|---|---|
| Rust 1.98.1 (edition 2024) | すべてのコード。`rust-toolchain.toml` で固定 |
| Cargo workspace | crateの管理。TuzuriではZedのワークスペースにChihayaburuのcrateを加えた |
| clang | Linux版の配布ビルド (WebRTCのヘッダーをGCC 11が通さないため) |
| ImageMagick | アイコンの加工 (外側の黒を透明に、各サイズのPNG・ICO) |

### UI
| 技術 | 用途 |
|---|---|
| [GPUI](https://github.com/zed-industries/zed) (Zedのrev `fae8a35`) | ChihayaburuのUIフレームワーク。GPUで描画する。Tuzuriでは同じリポジトリのものを使う |
| Zedの部品 (`workspace`・`project`・`editor`・`language`・`lsp`・`ui` など) | Tuzuriのタブ・ドックのパネル・エディタ・診断の表示 |

### PDF
| 技術 | 用途 |
|---|---|
| [hayro](https://crates.io/crates/hayro) 0.8 | PDFの解析とラスタライズ (純Rust) |
| [memmap2](https://crates.io/crates/memmap2) 0.9 | PDFとフォントをメモリマップで読む |
| [image](https://crates.io/crates/image) 0.25 | 描画した画像をGPUIの画像に変換 |
| [Tesseract](https://github.com/tesseract-ocr/tesseract) (外部コマンド) | 画像だけのPDFのOCR |

### Typst
| 技術 | 用途 |
|---|---|
| [typst](https://crates.io/crates/typst) 0.15 / `typst-layout` / `typst-pdf` | コンパイルとPDFの生成 |
| [typst-kit](https://crates.io/crates/typst-kit) 0.15 | ファイル・フォント・パッケージ (Typst Universeからのダウンロード)・日付 |
| [typst-ide](https://crates.io/crates/typst-ide) 0.15 | プレビューのクリック位置から、ソースの位置を引く |
| [comemo](https://crates.io/crates/comemo) 0.5 | Typstの差分コンパイルのキャッシュ (古いものを捨てるため) |
| [codebook-tree-sitter-typst](https://crates.io/crates/codebook-tree-sitter-typst) 0.12 (uben0/tree-sitter-typst、MIT) | Tuzuriでの色分け・アウトライン |

tinymist (Typstの言語サーバー) は使わない。別のプロセスで自前のコンパイラを持ち、メモリが二重になるため。

### ビルド・配布
| 技術 | 用途 |
|---|---|
| GitHub Actions | CI (`ci.yml`: fmt・clippy・テスト) と配布 (`release.yml`: Linux・Windows) |
| [sccache](https://github.com/mozilla/sccache) | 配布ビルドのコンパイル結果をActionsのキャッシュに置く |
| `softprops/action-gh-release` | タグのときにGitHub Releaseへ添付 |

## Chihayaburu

### pdf_core
- `PdfDocument`: PDFを**メモリマップで読む** (使ったページだけがメモリに載り、足りなくなればOSが捨てて読み直せる)。コンパイルしたばかりのPDFは `from_bytes` でメモリから開く。
- ページごとの文字・リンク・OCRの結果は、**初めて必要になったときに読み** (`OnceLock`)、その後は持ち続ける。
- 描画 (`render`) はキャッシュしない。画像は表示側 (`pdf_viewer`) だけが持つ (以前は文書側と二重に持っていて、155ページの資料で800MB近く使っていた)。
- リンク: 注釈 (`/Link`) と、文字の中のURLらしい部分 (`linkify`)。外部のURIは、安全なスキームだけを開く。
- OCR: ページを300dpi相当で描画し、Tesseractに標準入力で渡してTSVを受け取る。結果はメモリ上にだけ持つ。

### pdf_viewer
- `PdfView`: 全ページを縦に並べ、見えているページと前後1ページだけを描画する。
- **描画はバックグラウンドのスレッド**で行い、UIスレッドは画像を受け取って表示するだけ (1ページ十数〜数十msかかるため)。`images.rs` が、要求・結果・古い画像の仮表示・世代 (文書の入れ替え) を管理する。画面外に出たページの画像はGPUから捨てる。
- ズームの直後 (120ms) は再描画を待ち、連続したズームの途中で無駄に描かない。
- テキスト選択・コピー、リンク、目次とサムネイルのサイドバー、ツールバー、回転・白黒反転、ダークモード、OCR。
- 組み込み先への知らせ: Ctrl/Cmd+クリックで `PdfViewEvent::ModifiedClick { page, x, y }` (回転前のページ上の位置) を出す。Typstのプレビューが、ソースへのジャンプに使う。
- 文書の差し替え: `reload_document` (「Reloaded」を出す) と `update_document` (出さない。編集のたびの更新用)。どちらもスクロール位置を保つ。

### pdf_view_app (単体のアプリ)
- 自前のタイトルバー、PDFのタブ、最近開いたファイル、ファイルの選択とドロップ。
- 表示中のファイルが書き換わったら自動で読み直す。Typstのタブは、`import` したファイルも含めて見張る。
- Typstのタブ: 保存のたびにコンパイルし、PDFを一時フォルダに書いて表示する。エラーは下のパネルに出し、クリックで外部のエディタ (`PDF_VIEW_EDITOR`、Zed、VS Code) の該当行を開く。

## Tuzuri

### Zedの取り込み方
- Zedのリポジトリを**履歴ごとマージ**した (`zed` リモート)。上流の更新は `git fetch zed main` してmergeできる。
- Chihayaburuのcrateは、Zedのワークスペースに加えた。gpuiは同じリポジトリのものを使う。
- Zedのコードへの変更は最小にしている。主なもの:
  - 名前: 表示名・メニュー・設定とデータのフォルダ (`~/.config/tuzuri` など)・実行ファイル名 (`tuzuri`)。アプリID (`dev.zed.Zed-Dev`) とチャンネル (dev) はZedのまま。
  - 既定の設定: テレメトリを切る、拡張機能を自動で入れない (WebAssemblyの実行環境で約30MB増えるため)。
  - 起動処理: `pdf_tab`・`typst_ui` の登録、デバッグパネルの読み込み、`tuzuri export` の入口。
  - キーマップ (Linux・Windows・macOS)、エディタのツールバーのボタン。
- 配布ビルドでは `ZED_UPDATE_EXPLANATION` を付けて自動更新を止める (本家のZedで上書きされないように)。

### PDFのタブ (`pdf_tab`)
- Zedの画像ビューアと同じ形: `PdfItem` (プロジェクトの項目。開くときにバックグラウンドで読み込む) と `PdfTab` (タブ。中身はChihayaburuの `PdfView`)。
- Zedは、ファイルを開くとき後に登録したものから試すので、エディタより後に登録して `.pdf` を引き受ける。

### Typst

#### typst_build: コンパイル
- `FileWorld` (Typstの `World`):
  - **編集中の内容でディスクを上書きする**ファイルの読み込み (`OverlayFiles`)。保存していない内容でもコンパイルできる。
  - コンパイルの前に `FileStore::reset()` する。読み込んだソースはその場で書き換わり、変わっていない部分は前回の結果を使い回せる (差分コンパイル)。
  - フォント: 同梱のフォント (typst-assets) と、システムのフォント。システムのフォントは**メモリマップで読む** (`typst-kit` は丸ごと読み込むが、日本語のフォントは1つ20MBある)。
- `Session`: 1つの文書を、編集のたびにコンパイルし続ける。
  - `compile(overlays, export_pdf)`: PDFを作らずにエラーだけを調べることもできる (プレビューが無いとき)。
  - コンパイルのたびに `comemo::evict(10)` で、使われなくなったキャッシュを捨てる。
  - `source_at(page, x, y)`: プレビューの位置から、ソースのファイル・行・列を引く (`typst-ide`)。**最後に成功したときのソースを持っておき**、それで引く。Typstは書き換えのたびに位置の番号 (span) を振り直すので、今のソースでは合わない。
- `Diagnostic`: エラー・警告。ファイル名・行・列・ヒントと、エディタに印を付ける範囲 (行は0始まり、列はUTF-16の単位数。LSPと同じ数え方)。
- 書き出し (`export.rs`): 一時ファイルに書いてから名前を変える (失敗しても前のPDFが残る)。`tuzuri export` の引数の解釈と、端末への出力もここ。

#### typst_ui: Tuzuriの中のTypst
- **`TypstDocument`: ファイルごとに1つ、プロジェクトの中で共有する文書。** プレビューとデバッグパネルが同じコンパイラを使う。
  - 依存するファイル (メインと `import` したもの) のうち、開いているバッファの編集を見張る。編集が**0.3秒止まったら**、バックグラウンドでコンパイルする。コンパイル中に編集が来たら、終わってからもう一度コンパイルする。
  - PDFは、プレビューが求めている間 (`PdfRequest` が生きている間) だけ作って持つ。プレビューが無ければ、エラーと警告だけを調べ、PDFは捨てる。
  - `import` される側のファイルは、それを読み込んでいる文書に属するとみなす (`for_file`)。
- **プレビュー** (`TypstPreview`): 文書の最新のPDFを `PdfView` で表示する。エラーの間は最後に成功した表示を残し、上部に1行で出す。Ctrl/Cmd+クリックで、プレビューの無いペインでソースを開き、カーソルを移す (Typstの列は文字数、Zedはバイト数なので変換する)。
- **デバッグモード** (`TypstDiagnosticsPanel`): ドックのパネル。表示されている間だけ、アクティブなエディタの文書を持ち、ERROR/WARNINGを一覧する。行のクリックでその位置へ移る。隠すと文書を手放す (コンパイラも解放される)。
- **エディタの波線** (`EditorDiagnostics`): デバッグモードの間、Typstの診断をLSPの形にして、Zedの `LspStore::merge_lsp_diagnostics` に渡す。言語サーバーの診断と同じく、波線・ホバー・Project Diagnostics・ステータスバーの件数に出る。実在しないサーバーのID (`usize::MAX / 2`) の下に置く。
- **書き出し**: 未保存の内容で、メインのファイルの隣にPDFを書き、通知を出す (「Open」でTuzuriの中に開く)。
- 操作: エディタのツールバーのボタン (プレビュー・書き出し)、`Ctrl+K V` (横にプレビュー)、`Ctrl+Shift+V`、コマンドパレット (`typst: open preview` など)、ステータスバーのデバッグパネルのボタン。

#### 色分け (`crates/grammars/src/typst`)
- 組み込みの言語「Typst」(言語サーバーは無し)。色分け・コードブロックの中の言語・見出しのアウトライン・括弧の対応・コメントの切り替え。
- 文法に付いていた定義 (Helix向け) を、Zedのテーマの名前に合わせて書き直した。Zedは後のパターンが優先なので、一般的なものを先に置く。

### 編集してからプレビューが変わるまで

```
エディタで入力
  → バッファの Edited イベント (TypstDocument が見張っている)
  → 0.3秒待つ (その間の入力でやり直し)
  → 未保存のバッファの内容を集める
  → バックグラウンド: Session::compile(未保存の内容, PDFを作るか)
        FileStore::reset → typst::compile (変わっていない部分は使い回す)
        → PDF (typst-pdf) → PdfDocument::from_bytes
        → comemo::evict(10)
  → UIスレッド: エラー・警告・PDFを更新し、TypstDocumentEvent::Updated
        ├─ プレビュー: PdfView::update_document (スクロール位置を保つ)
        └─ デバッグパネル: 一覧と、エディタの波線を更新
```

### コマンドライン
`tuzuri export <入力.typ> [-o <出力>] [--format pdf]`: 最初の引数が `export` なら、Zedの引数の解釈より前に処理して終わる。GUIもZedの設定も読み込まない。エラーは `file:line:column: error: message` で出し、終了コードは成功0・文書のエラー1・引数の誤り2。Windowsでは、起動した端末に `AttachConsole` して出力する。

## メモリの方針
測ってから決めた。メモリは、**アプリだけが持つ分 (RssAnon)** と、OSが捨てて読み直せる**ファイルの分 (RssFile)** を分けて測る。

| 工夫 | 効果 (実測) |
|---|---|
| PDFをメモリマップで読み、描画結果を二重に持たない (Chihayaburu) | 155ページの写真入りPDFで 339MB → 約248MB |
| システムのフォントをメモリマップで読む | 日本語40章の書き出しで RssAnon 89MB → 38MB |
| 拡張機能を自動で入れない (Tuzuriの既定) | `.typ` を開いた起動直後の RssAnon 86MB → 57MB (release) |
| PDFは、プレビューが求めている間だけ作る | デバッグパネルだけのとき、PDFを作らず持たない |
| コンパイラは文書ごとに1つを共有し、使わなくなったら解放する | プレビューとパネルの両方を開いても増えない |
| Typstのキャッシュを毎回捨てる (`comemo::evict(10)`) | 編集を続けても増え続けない |
| tinymistを使わない | 別のプロセスのコンパイラを持たない |

`tuzuri export` は、配布ビルドで日本語の文書を書き出して RssAnon の最大20MB。Tuzuri全体の残り約55MBは、Zed本体の分。

## テスト
- `pdf_core`・`typst_build`: 単体のテスト (実際のPDFのfixture、一時フォルダのTypstの文書、実物のTesseract)。
- `pdf_viewer`: 計算だけの部分の単体テストと、GPUIのテスト (ウィンドウを作り、クリックやズームを送る)。
- `typst_ui`: **Zedのテスト環境でエディタ・プレビュー・パネルを実際に動かす**テスト (未保存の編集の反映、エラー時に古い表示が残る、Ctrl+クリックでの移動、パネルの一覧と波線、書き出し)。コンパイラはディスクを読むので、ファイルは一時フォルダとZedの偽のファイルシステムの両方に書く。
- Tuzuri本体: Zedのテスト (キーマップのアクションの名前空間、既定の設定の読み込みなど)。
- CI (`ci.yml`): 対象のcrateだけを fmt・clippy (`--no-deps`)・テスト。**テスト用の機能なしでの型検査**も行う (テストでだけ使える関数を本体で使っていないか)。

## ビルドと配布
- 開発用ビルド: デバッグ情報はファイルと行だけ (`line-tables-only`)。変数まで見たいときは `--profile dbg`。
- 配布 (`.github/workflows/release.yml`): `v*` のタグでLinux (tar.gz) とWindows (zip) を作り、GitHub Releaseに添付する。手動実行では対象のOSを選べ、成果物として置く。
  - デバッグ情報なし、Zedの `.cargo/bundle-config.toml` (rpathと、サイズを優先した最適化)。
  - Linux: Ubuntu 22.04・clang。基本のもの以外の共有ライブラリを同梱する。ただし `libstdc++` と `libxkbcommon` はシステムのものを使う (古いものを同梱すると、GPUのドライバやキーボードの設定と合わない)。`install.sh` で `~/.local` に入れる。
  - Windows: `tuzuri.exe` とREADMEのzip。
  - まとめたものを展開し、`tuzuri export` でPDFができることを確かめてから添付する。
  - sccacheでコンパイル結果をActionsのキャッシュに置く。キャッシュはリポジトリ全体で10GBまでなので、sccacheの分だけにしている。

## 既知の制約
- Tuzuriのプレビュー・デバッグパネル・ボタンの見た目と操作は、実際のウィンドウでは十分に確かめていない (テスト環境での確認が中心)。
- Zedで開いていないファイル (画像や、開いていない `import` 先) をディスク上で書き換えても、プレビューは更新されない。
- 補完・ホバーなどの編集支援 (tinymistの機能) は無い。
- 単体アプリ (`pdf_view_app`) のTypst連携は、保存のたびのコンパイル (一時ファイル経由) のまま。
- レポートモードと Agentic Peer Review、LaTeX・EPUBは未着手。
