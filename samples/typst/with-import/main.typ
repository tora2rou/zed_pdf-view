// 別ファイルを読み込むサンプル。lib.typを保存し直しても、自動で更新される。
#import "lib.typ": greeting, accent

#set page(width: 12cm, height: auto, margin: 1cm)
#set text(lang: "ja")

= #greeting("Chihayaburu")

lib.typ の色を書き換えて保存すると、この文書の色が変わる。

#accent[強調したい部分]
