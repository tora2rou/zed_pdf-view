#!/usr/bin/env python3
"""サンプルのEPUB (sample.epub) を作る。使い方: python3 make_sample.py"""
import struct
import zipfile
import zlib
from pathlib import Path

OUT = Path(__file__).with_name("sample.epub")


def png(width: int, height: int) -> bytes:
    """色が変わっていく画像 (PNG) を作る。"""
    rows = b""
    for y in range(height):
        rows += b"\x00" + b"".join(
            bytes((x * 255 // width, y * 255 // height, 160)) for x in range(width)
        )

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )


def page(title: str, body: str) -> str:
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="ja"><head><title>{title}</title></head>
<body>
{body}
</body></html>
"""


CHAPTERS = {
    "cover.xhtml": page(
        "表紙",
        '<h1 id="top">EPUBプレビューのサンプル</h1>\n<p>Chihayaburu で EPUB を開いたときの表示を確かめる本です。</p>\n'
        '<p><img src="images/gradient.png" alt="グラデーション"/></p>\n'
        '<p><a href="chapter1.xhtml">第一章へ</a> / <a href="chapter2.xhtml#table">第二章の表へ</a></p>',
    ),
    "chapter1.xhtml": page(
        "第一章",
        '<h1 id="ch1">第一章 吾輩は猫である</h1>\n'
        "<p>吾輩は<ruby>猫<rp>(</rp><rt>ねこ</rt><rp>)</rp></ruby>である。名前はまだ無い。\n"
        "どこで生れたかとんと<ruby>見当<rt>けんとう</rt></ruby>がつかぬ。何でも薄暗いじめじめした所で\n"
        "ニャーニャー泣いていた事だけは記憶している。</p>\n"
        "<h2>英語の段落</h2>\n"
        "<p>This chapter mixes <em>emphasis</em>, <strong>strong text</strong>, and a "
        '<a href="https://example.com/">web link</a>. Special characters such as # $ * _ and -- are shown as they are.</p>\n'
        "<blockquote><p>引用のブロック。Quoted text is indented.</p></blockquote>\n"
        '<p><a href="#ch1">この章の先頭へ</a> / <a href="cover.xhtml#top">表紙へ</a></p>',
    ),
    "chapter2.xhtml": page(
        "第二章",
        '<h1>第二章 リストと表</h1>\n'
        "<h2>リスト</h2>\n"
        "<ul><li>箇条書きの項目<ul><li>入れ子の項目</li></ul></li><li>もう一つの項目</li></ul>\n"
        '<ol start="3"><li>三番目から始まる番号つきの項目</li><li>四番目</li></ol>\n'
        '<h2 id="table">表</h2>\n'
        "<table><tr><th>機能</th><th>状態</th></tr>"
        "<tr><td>EPUB</td><td>プレビュー</td></tr>"
        '<tr><td colspan="2">セルの結合も使える</td></tr></table>\n'
        "<pre>preformatted\n  text keeps\n    its layout</pre>\n"
        "<p>画像が壊れていても、本全体は組版される。</p>",
    ),
}

CONTAINER = """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>
"""


def opf() -> str:
    items = "".join(
        f'<item id="c{i}" href="{name}" media-type="application/xhtml+xml"/>'
        for i, name in enumerate(CHAPTERS)
    )
    refs = "".join(f'<itemref idref="c{i}"/>' for i in range(len(CHAPTERS)))
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">chihayaburu-sample</dc:identifier>
    <dc:title>EPUBプレビューのサンプル</dc:title>
    <dc:creator>Chihayaburu</dc:creator>
    <dc:language>ja</dc:language>
  </metadata>
  <manifest>{items}<item id="img" href="images/gradient.png" media-type="image/png"/></manifest>
  <spine>{refs}</spine>
</package>
"""


def main() -> None:
    with zipfile.ZipFile(OUT, "w") as z:
        # mimetype は先頭で、圧縮しない (EPUBの決まり)。
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", CONTAINER, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/content.opf", opf(), compress_type=zipfile.ZIP_DEFLATED)
        for name, html in CHAPTERS.items():
            z.writestr(f"OEBPS/{name}", html, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/images/gradient.png", png(480, 240), compress_type=zipfile.ZIP_DEFLATED)
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
