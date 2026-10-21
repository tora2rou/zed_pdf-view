"""16〜256pxのPNGを、1つの.icoファイルにまとめる(各サイズはPNGのまま格納。Windows Vista以降で表示できる)。
使い方: python3 make_ico.py <PNGのあるフォルダ> <出力する.ico>
"""
import struct
import sys

SIZES = [16, 24, 32, 48, 64, 128, 256]


def main(src_dir, out_path):
    images = [open(f"{src_dir}/icon_{s}.png", "rb").read() for s in SIZES]
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    for size, data in zip(SIZES, images):
        # 幅・高さの256は0で表す
        dim = 0 if size == 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    with open(out_path, "wb") as f:
        f.write(header + entries + b"".join(images))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
