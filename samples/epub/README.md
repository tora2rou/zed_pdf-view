# EPUBのサンプル

```sh
cargo run --release -p pdf_view_app -- samples/epub/sample.epub
```

`sample.epub` は `make_sample.py` で作ったもの (`python3 make_sample.py` で作り直せる)。
日本語とルビ、英語、強調、リスト(入れ子・番号の開始)、表(セルの結合)、引用、整形済みテキスト、
本の中のリンク(章をまたぐものも)、画像を含む。EPUBはTypstに変換して組版するので、
縦書きとCSSには対応しない。
