// 長い文書のサンプル(約40ページ)。スクロールバー、サムネイル、目次(Outline)、再コンパイルの速さを試す。
#set page(paper: "a4", margin: 2.5cm, numbering: "1")
#set text(size: 11pt, lang: "ja")
#set heading(numbering: "1.1")

#outline(title: "目次", depth: 2)
#pagebreak()

#for chapter in range(1, 9) [
  = 第#chapter 章

  #for section in range(1, 5) [
    == 第#chapter.#section 節

    #lorem(220)

    #lorem(180)
  ]

  #pagebreak(weak: true)
]
