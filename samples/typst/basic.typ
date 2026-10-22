// 基本のサンプル。日本語、見出し、数式、表、リスト。
// 保存し直すと、開いているChihayaburuが自動で更新される。
#set page(paper: "a4", margin: 2.5cm, numbering: "1")
#set text(size: 11pt, lang: "ja")
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Typstのプレビュー]\
  #v(4pt)
  日本語と数式の組版テスト
]

= はじめに

これはChihayaburuで`.typ`ファイルを開いたときの表示を確かめるためのサンプルです。
ファイルを書き換えて保存すると、約1秒で表示が更新されます。

== 箇条書き

- 見出しと番号
- 日本語の本文
- 数式と表

== 数式

二次方程式の解の公式:
$ x = (-b plus.minus sqrt(b^2 - 4 a c)) / (2 a) $

和の公式: $ sum_(k=1)^n k = (n (n + 1)) / 2 $

== 表

#table(
  columns: (1fr, 1fr, 1fr),
  align: center,
  [*機能*], [*状態*], [*備考*],
  [スクロールバー], [済], [本文とサイドバー],
  [タブ], [済], [Ctrl+Tabで切り替え],
  [OCR], [済], [Tesseractが必要],
  [Typst], [済], [このファイル],
)

= 長めの本文

#lorem(180)

#lorem(160)

#pagebreak()

= 2ページ目

ここから2ページ目です。ページ番号が下に出ていることを確かめてください。

#lorem(120)
