# アイコン

アプリ「Chihayaburu」のアイコン(かるた)。赤い札の上に生成りの取り札を重ね、明朝体の「ち」(ちはやぶる)を置いている。小さいサイズ(48px以下)では、細い線が消えないよう文字を大きく・太くしている。

## 作り直し方

```sh
cd packaging/icon
cargo run --release -- /usr/share/fonts/opentype/ipaexfont-mincho/ipaexm.ttf /tmp/icons
python3 make_ico.py /tmp/icons ../../crates/pdf_view_app/assets/chihayaburu.ico
cp /tmp/icons/icon_256.png ../../crates/pdf_view_app/assets/chihayaburu.png
```

文字の形は IPAex明朝 (IPAフォントライセンス) から取っている。
