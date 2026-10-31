# Typstのサンプル

```sh
cargo run --release -p pdf_view_app -- samples/typst/basic.typ
```

| ファイル | 試すこと |
|---|---|
| `basic.typ` | 日本語、見出し、数式、表。書き換えて保存すると約1秒で更新される |
| `long.typ` | 約40ページ。スクロールバー、サムネイル、目次(Outline)、再コンパイルの速さ |
| `broken.typ` | 評価エラー。1件ずつ出る。エラーの行をクリックするとエディタで開く。直すと次のエラーが出る |
| `broken-syntax.typ` | 構文エラー。複数が一度に、ヒントつきで出る |
| `with-import/main.typ` | 別ファイル(`lib.typ`)を読み込む。`lib.typ`を保存し直しても更新される |

エディタを変えるとき: `PDF_VIEW_EDITOR='code --goto {file}:{line}:{column}'`
