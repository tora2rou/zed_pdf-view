# LaTeXのサンプル

`latexmk`(TeX Live)が必要。

```sh
cargo run --release -p pdf_view_app -- samples/latex/basic.tex
```

| ファイル | 試すこと |
|---|---|
| `basic.tex` | pdfLaTeX。数式、表、2ページ。保存すると自動で更新される |
| `japanese.tex` | LuaLaTeX + LuaTeX-ja の日本語。`% !TEX program = lualatex` でエンジンを指定 |
| `broken.tex` | エラーが画面の下に出る。エラーの行をクリックするとエディタで開く |
| `with-input/main.tex` | `\input` した `part.tex` を保存し直しても更新される |

エンジンは、先頭付近の `% !TEX program = lualatex`(`pdflatex` `xelatex` `lualatex` `platex` `uplatex`)で指定する。
なければ、クラス(`ltjsarticle` `jsarticle` など)やパッケージから推定し、それも無ければpdfLaTeX。
