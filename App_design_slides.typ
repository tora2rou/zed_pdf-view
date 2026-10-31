// Chihayaburu・Tuzuri の設計 (App_design.md のスライド版)
// 書き出し: tuzuri export App_design_slides.typ

#let accent = rgb("#2f3550")
#let soft = rgb("#f6f1e6")
#let muted = rgb("#6b6f80")
#let highlight = rgb("#c0603a")

#set page(
  paper: "presentation-16-9",
  margin: (x: 1.6cm, top: 1.4cm, bottom: 1.2cm),
  fill: white,
  footer: context {
    set text(size: 10pt, fill: muted)
    let n = counter(page).get().first()
    if n > 1 [
      Chihayaburu・Tuzuri の設計
      #h(1fr)
      #n / #counter(page).final().first()
    ]
  },
)
#set text(font: ("Noto Sans CJK JP", "DejaVu Sans"), size: 19pt, lang: "ja", fill: accent)
#set par(leading: 0.75em)
#set list(spacing: 0.7em, marker: text(fill: highlight)[▸])
#set table(stroke: 0.5pt + muted.lighten(50%), inset: 7pt)
#show table.cell.where(y: 0): set text(weight: "bold")
#show raw: set text(font: ("DejaVu Sans Mono", "Noto Sans CJK JP"), size: 1.05em)

// 1枚のスライド。見出しと本文。
#let slide(title, body) = {
  pagebreak(weak: true)
  block(width: 100%, below: 0.8em)[
    #text(size: 30pt, weight: "bold")[#title]
    #v(-0.4em)
    #line(length: 100%, stroke: 2pt + highlight)
  ]
  set text(size: 19pt)
  body
}

// 図の中の箱。
#let node(body, fill: soft) = box(
  fill: fill,
  stroke: 1pt + accent,
  radius: 5pt,
  inset: (x: 10pt, y: 8pt),
  text(size: 17pt, body),
)

// 小さな注記。
#let note(body) = text(size: 16pt, fill: muted, body)

// ───────────── 表紙 ─────────────
#page(fill: soft, footer: none)[
  #v(1fr)
  #text(size: 40pt, weight: "bold")[Chihayaburu・Tuzuri の設計]
  #v(0.2em)
  #line(length: 60%, stroke: 3pt + highlight)
  #v(0.6em)
  #text(size: 20pt)[PDFビューアと、Zedをforkした組版エディタ]
  #v(1fr)
  #text(size: 14pt, fill: muted)[2026-10-31 · ブランチ `Tuzuri` · 詳しくは App_design.md]
]

#slide[2つのアプリ][
  #grid(
    columns: (1fr, 1fr),
    gutter: 1.2cm,
    [
      #text(size: 22pt, weight: "bold")[Chihayaburu]
      - GPUI製のPDFビューア
      - 単体のアプリ (`pdf_view_app`)
      - 表示部分はライブラリ (`pdf_viewer`) として組み込める
      - `.typ` を開くとコンパイルして表示
    ],
    [
      #text(size: 22pt, weight: "bold")[Tuzuri]
      - Zed (コードエディタ) のfork
      - Chihayaburuを組み込んだ組版エディタ
      - Typstを編集しながら、アプリの中で\
        プレビュー・エラー表示・PDFの書き出し
    ],
  )
  #v(1fr)
  #align(center, block(fill: soft, inset: 12pt, radius: 6pt)[
    設計の目的 (design.md) で最も大きいのは *少ないメモリで動くこと*
  ])
]

#slide[全体の構成][
  #set align(center)
  #scale(82%, reflow: true)[
  #block(stroke: (dash: "dashed", paint: muted), inset: 10pt, radius: 6pt)[
    #text(size: 16pt, fill: muted)[Tuzuri (Zedのfork)] \
    #node(fill: rgb("#e8ecf6"))[`zed` (実行ファイル `tuzuri`)]
    #h(0.4em) ← #h(0.4em)
    #node(fill: rgb("#e8ecf6"))[`typst_ui`]
    #node(fill: rgb("#e8ecf6"))[`pdf_tab`]
    #node(fill: rgb("#e8ecf6"))[`grammars/typst`]
    #node(fill: rgb("#e8ecf6"))[Zedの部品]
  ]
  #v(0.2em) ↓ #v(0.2em)
  #block(stroke: (dash: "dashed", paint: muted), inset: 10pt, radius: 6pt)[
    #text(size: 16pt, fill: muted)[Chihayaburu] \
    #node[`pdf_view_app` (単体アプリ)]
    #h(0.4em) → #h(0.4em)
    #node[`pdf_viewer` (GPUIの表示部品)]
    #h(0.4em) → #h(0.4em)
    #node[`pdf_core` (GUI非依存)]
    #v(0.3em)
    #node[`typst_build` (Typst → PDF、GUI非依存)]
  ]
  ]
  #v(0.4em)
  #set align(left)
  - GUI非依存の処理・計算を分ける → *テストしやすく、組み込みやすい*
  - Chihayaburuの部品はZedに依存しない → 単体アプリとTuzuriで共用
]

#slide[crateの役割][
  #set text(size: 17pt)
  #table(
    columns: (auto, 1fr, auto, auto),
    table.header[crate][役割][UI][規模],
    [`pdf_core`], [PDFの読み込み・ラスタライズ・文字・リンク・目次・OCR], [なし], [約2,100行],
    [`pdf_viewer`], [`PdfView`: スクロール・ズーム・選択・サイドバー・ツールバー], [GPUI], [約5,400行],
    [`pdf_view_app`], [単体アプリ: ウィンドウ・タブ・自動再読み込み・Typstのタブ], [GPUI], [約2,600行],
    [`typst_build`], [Typstのコンパイル・編集中の内容でのコンパイル・エラー位置・書き出し], [なし], [約1,100行],
    [`pdf_tab`], [Tuzuriで `.pdf` をタブとして開く], [Zed], [約190行],
    [`typst_ui`], [共有の文書・プレビュー・デバッグパネル・波線・書き出し], [Zed], [約1,700行],
  )
]

#slide[使用した技術 (1): UI・PDF・Typst][
  #set text(size: 15pt)
  #grid(
    columns: (1fr, 1fr),
    gutter: 0.8cm,
    table(
      columns: (auto, 1fr),
      table.header[技術][用途],
      [Rust 1.98.1], [すべてのコード (edition 2024)],
      [GPUI], [UI。GPUで描画 (Zedのrev `fae8a35`)],
      [Zedの部品], [タブ・パネル・エディタ・診断],
      [hayro 0.8], [PDFの解析とラスタライズ (純Rust)],
      [memmap2 0.9], [PDFとフォントをメモリマップで読む],
      [Tesseract], [画像だけのPDFのOCR (外部コマンド)],
    ),
    table(
      columns: (auto, 1fr),
      table.header[技術][用途],
      [typst 0.15], [コンパイルとPDFの生成 (+ typst-pdf)],
      [typst-kit], [ファイル・フォント・パッケージ],
      [typst-ide], [プレビューの位置 → ソースの位置],
      [comemo], [差分コンパイルのキャッシュの管理],
      [tree-sitter-typst], [色分け・アウトライン (MIT)],
    ),
  )
  #v(0.5em)
  #note[tinymist (言語サーバー) は使わない: 別プロセスでコンパイラを持ち、メモリが二重になるため]
]

#slide[使用した技術 (2): ビルド・配布][
  #table(
    columns: (auto, 1fr),
    table.header[技術][用途],
    [Cargo workspace], [crateの管理。Zedのワークスペースに Chihayaburu のcrateを追加],
    [GitHub Actions], [CI (fmt・clippy・テスト) と配布 (Linux tar.gz・Windows zip)],
    [sccache], [配布ビルドのコンパイル結果を Actions のキャッシュに置く],
    [clang], [Linuxの配布ビルド (GCC 11 が WebRTC のヘッダーを通さない)],
    [action-gh-release], [`v*` のタグで GitHub Release に添付],
    [ImageMagick], [アイコンの加工 (外側を透明に、PNG・ICO)],
  )
]

#slide[Chihayaburu: pdf_core と pdf_viewer][
  #grid(
    columns: (1fr, 1fr),
    gutter: 1cm,
    [
      #text(weight: "bold")[pdf_core]
      - PDFを *メモリマップ* で読む\
        #note[使ったページだけがメモリに載る]
      - 文字・リンク・OCRの結果は\
        初めて必要になったときに読む
      - 描画結果はキャッシュしない\
        #note[表示側だけが持つ (二重に持たない)]
      - リンク: 注釈 + 文字の中のURL
    ],
    [
      #text(weight: "bold")[pdf_viewer (`PdfView`)]
      - 見えているページ ± 1ページだけ描画
      - *描画はバックグラウンドのスレッド*\
        #note[UIスレッドは受け取って表示するだけ]
      - 画面外のページの画像はGPUから捨てる
      - ズーム直後 120ms は再描画を待つ
      - Ctrl+クリックの位置を組み込み先へ\
        (`PdfViewEvent::ModifiedClick`)
    ],
  )
]

#slide[Tuzuri: Zed の取り込み方][
  - Zedのリポジトリを *履歴ごとマージ* → 上流の更新を `git fetch zed main` で取り込める
  - Zedのコードへの変更は最小に
    - 名前: 表示名・メニュー・設定フォルダ (`~/.config/tuzuri`)・実行ファイル名 `tuzuri`
    - 既定の設定: *テレメトリを切る*、*拡張機能を自動で入れない* (約30MB減)
    - 起動処理: `pdf_tab`・`typst_ui`・デバッグパネルの登録、`tuzuri export` の入口
    - キーマップ、エディタのツールバーのボタン
  - 配布ビルドは自動更新を止める (本家のZedで上書きされない)
  - `.pdf` は Zed の画像ビューアと同じ形のタブ (`pdf_tab`) で開く
]

#slide[Tuzuri: Typst のコンパイル (typst_build)][
  - *編集中の内容でディスクを上書き* して読む (`OverlayFiles`) → 保存せずにコンパイル
  - `FileStore::reset()` で読み込み直し → 変わっていない部分は *前回の結果を使い回す*
  - `Session`: 1つの文書を編集のたびにコンパイルし続ける
    - PDFを作らずエラーだけ調べることもできる
    - 毎回 `comemo::evict(10)` で古いキャッシュを捨てる
    - `source_at`: プレビューの位置 → ソースの行・列\
      #note[成功したときのソースで引く (Typstは書き換えのたびに位置の番号を振り直す)]
  - システムのフォントは *メモリマップ* で読む (日本語のフォントは1つ20MB)
]

#slide[Tuzuri: typst_ui][
  #grid(
    columns: (1fr, 1fr),
    gutter: 1cm,
    [
      #text(weight: "bold")[TypstDocument (共有の文書)]
      - ファイルごとに1つを共有
      - 編集が *0.3秒* 止まったらコンパイル
      - PDFはプレビューがある間だけ作る
      - `import` される側は読み込む文書へ
    ],
    [
      #text(weight: "bold")[それを表示するもの]
      - *プレビュー*: Ctrl+クリックでソースへ
      - *デバッグパネル*: エラーと警告の一覧
      - *エディタの波線*: LSPの診断として渡す
      - *書き出し*: 未保存の内容でPDFに
    ],
  )
  #v(0.3em)
  #note[操作: ツールバーのボタン・`Ctrl+K V`・コマンドパレット・ステータスバー]
]

#slide[編集してからプレビューが変わるまで][
  #set text(size: 18pt)
  #let step(body) = node(body)
  #let arrow = align(center, text(fill: highlight)[↓])
  #grid(
    columns: (1fr, 1.2fr),
    gutter: 0.8cm,
    [
      #step[エディタで入力]
      #arrow
      #step[バッファの `Edited` (TypstDocumentが見張る)]
      #arrow
      #step[0.3秒待つ (その間の入力でやり直し)]
      #arrow
      #step[未保存のバッファの内容を集める]
    ],
    [
      #step[*バックグラウンド*: `Session::compile` \
        reset → typst::compile (使い回し) → PDF \
        → `PdfDocument::from_bytes` → evict]
      #arrow
      #step[*UIスレッド*: エラー・警告・PDFを更新 \
        → `TypstDocumentEvent::Updated`]
      #arrow
      #step[プレビュー: `update_document` (位置を保つ) \
        デバッグパネル: 一覧と波線を更新]
    ],
  )
]

#slide[メモリの方針 (実測)][
  #set text(size: 17pt)
  #note[アプリだけが持つ分 (RssAnon) と、OSが捨てられるファイルの分 (RssFile) を分けて測る]
  #v(0.2em)
  #table(
    columns: (1fr, auto),
    table.header[工夫][効果],
    [PDFをメモリマップで読み、描画結果を二重に持たない], [155ページで 339MB → 約248MB],
    [システムのフォントをメモリマップで読む], [日本語40章の書き出し 89MB → *38MB*],
    [拡張機能を自動で入れない (Tuzuriの既定)], [起動直後 86MB → *57MB*],
    [PDFはプレビューが求めている間だけ作る], [パネルだけならPDFを持たない],
    [コンパイラは文書ごとに1つを共有・使わなければ解放], [プレビュー + パネルでも増えない],
    [Typstのキャッシュを毎回捨てる], [編集を続けても増え続けない],
    [tinymistを使わない], [別プロセスのコンパイラを持たない],
  )
  #v(0.2em)
  #note[`tuzuri export` (配布ビルド、日本語の文書): 最大 20MB。Tuzuri全体の残り約55MBは Zed本体の分。]
]

#slide[テスト][
  - `pdf_core`・`typst_build`: 実際のPDF、一時フォルダのTypstの文書、実物のTesseract
  - `pdf_viewer`: 計算の単体テスト + GPUIのテスト (クリック・ズームを送る)
  - `typst_ui`: *Zedのテスト環境でエディタ・プレビュー・パネルを実際に動かす*
    - 未保存の編集の反映、エラー時に古い表示が残る、Ctrl+クリックでの移動
    - パネルの一覧と波線 (Zedのエラー件数 1 → 0)、書き出し
  - CI: 対象のcrateだけ fmt・clippy・テスト
    - *テスト用の機能なしでの型検査* も行う\
      #note[テストでだけ使える関数を本体で使い、本体がビルドできなかったことがあった]
]

#slide[ビルドと配布][
  - 開発用: デバッグ情報はファイルと行だけ → ビルド 10分19秒 → 6分11秒
  - 配布 (`release.yml`): タグ `v*` で Linux と Windows を作り、Release に添付
    - Linux: Ubuntu 22.04 + clang、共有ライブラリを同梱して `install.sh`
      - `libstdc++`・`libxkbcommon` は *システムのもの* を使う\
        #note[古いものを同梱すると GPU のドライバと合わず「Failed to create surface」]
    - Windows: `tuzuri.exe` の zip
    - 展開して `tuzuri export` でPDFができることを確かめてから添付
  - sccache: コンパイル結果を Actions のキャッシュに (リポジトリ全体で10GBまで)
]

#slide[既知の制約・これから][
  - プレビュー・パネル・ボタンの見た目と操作は、実際のウィンドウでは十分に確かめていない
  - Zedで開いていないファイルをディスク上で書き換えても、プレビューは更新されない
  - 補完・ホバーなどの編集支援 (tinymistの機能) は無い
  - 単体アプリのTypst連携は、保存のたびのコンパイル (一時ファイル経由) のまま
  - これから: レポートモードと Agentic Peer Review、LaTeX・EPUB
]
