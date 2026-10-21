//! ページ画像の表示用変換(回転・白黒反転)。GPUIに依存しない純粋関数。

use pdf_core::RenderedPage;

/// 表示上の回転(時計回り)。PDFファイル自体は変更しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Rotation {
    #[default]
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    pub fn clockwise(self) -> Self {
        match self {
            Self::Deg0 => Self::Deg90,
            Self::Deg90 => Self::Deg180,
            Self::Deg180 => Self::Deg270,
            Self::Deg270 => Self::Deg0,
        }
    }

    pub fn counterclockwise(self) -> Self {
        match self {
            Self::Deg0 => Self::Deg270,
            Self::Deg90 => Self::Deg0,
            Self::Deg180 => Self::Deg90,
            Self::Deg270 => Self::Deg180,
        }
    }

    /// 縦横が入れ替わる回転(90度/270度)か。
    pub fn swaps_axes(self) -> bool {
        matches!(self, Self::Deg90 | Self::Deg270)
    }

    /// 回転後のページサイズ(幅, 高さ)。
    pub fn apply_to_size(self, (width, height): (f32, f32)) -> (f32, f32) {
        if self.swaps_axes() {
            (height, width)
        } else {
            (width, height)
        }
    }
}

/// 回転・反転した新しい画像を作る。何も変えないときもコピーを返す。
pub fn transform(page: &RenderedPage, rotation: Rotation, invert: bool) -> RenderedPage {
    let mut out = rotate(page, rotation);
    if invert {
        invert_colors(&mut out.rgba);
    }
    out
}

fn rotate(page: &RenderedPage, rotation: Rotation) -> RenderedPage {
    let (w, h) = (page.width as usize, page.height as usize);
    if rotation == Rotation::Deg0 {
        return page.clone();
    }
    let (out_w, out_h) = if rotation.swaps_axes() {
        (h, w)
    } else {
        (w, h)
    };
    let mut rgba = vec![0u8; page.rgba.len()];
    for y in 0..h {
        for x in 0..w {
            // 元の画素(x, y)の回転後の位置
            let (nx, ny) = match rotation {
                Rotation::Deg0 => (x, y),
                Rotation::Deg90 => (h - 1 - y, x),
                Rotation::Deg180 => (w - 1 - x, h - 1 - y),
                Rotation::Deg270 => (y, w - 1 - x),
            };
            let src = (y * w + x) * 4;
            let dst = (ny * out_w + nx) * 4;
            rgba[dst..dst + 4].copy_from_slice(&page.rgba[src..src + 4]);
        }
    }
    RenderedPage {
        width: out_w as u32,
        height: out_h as u32,
        rgba,
    }
}

/// RGBを反転する。アルファは変えない。
fn invert_colors(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        px[0] = 255 - px[0];
        px[1] = 255 - px[1];
        px[2] = 255 - px[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x1の画像: 左が赤、右が青。
    fn sample() -> RenderedPage {
        RenderedPage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        }
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    #[test]
    fn rotation_cycles() {
        let mut r = Rotation::Deg0;
        for _ in 0..4 {
            r = r.clockwise();
        }
        assert_eq!(r, Rotation::Deg0);
        assert_eq!(Rotation::Deg0.counterclockwise(), Rotation::Deg270);
        for r in [
            Rotation::Deg0,
            Rotation::Deg90,
            Rotation::Deg180,
            Rotation::Deg270,
        ] {
            assert_eq!(r.clockwise().counterclockwise(), r);
        }
    }

    #[test]
    fn size_swaps_only_for_quarter_turns() {
        assert_eq!(Rotation::Deg0.apply_to_size((2.0, 3.0)), (2.0, 3.0));
        assert_eq!(Rotation::Deg90.apply_to_size((2.0, 3.0)), (3.0, 2.0));
        assert_eq!(Rotation::Deg180.apply_to_size((2.0, 3.0)), (2.0, 3.0));
        assert_eq!(Rotation::Deg270.apply_to_size((2.0, 3.0)), (3.0, 2.0));
    }

    #[test]
    fn rotate_90_puts_left_pixel_on_top() {
        // 時計回り90度: 横並び(赤,青)が縦並び(上:赤, 下:青)になる
        let out = transform(&sample(), Rotation::Deg90, false);
        assert_eq!((out.width, out.height), (1, 2));
        assert_eq!(out.rgba, [RED, BLUE].concat());
    }

    #[test]
    fn rotate_180_reverses_order() {
        let out = transform(&sample(), Rotation::Deg180, false);
        assert_eq!((out.width, out.height), (2, 1));
        assert_eq!(out.rgba, [BLUE, RED].concat());
    }

    #[test]
    fn rotate_270_puts_left_pixel_on_bottom() {
        let out = transform(&sample(), Rotation::Deg270, false);
        assert_eq!((out.width, out.height), (1, 2));
        assert_eq!(out.rgba, [BLUE, RED].concat());
    }

    #[test]
    fn four_quarter_turns_restore_the_image() {
        let mut page = sample();
        for _ in 0..4 {
            page = transform(&page, Rotation::Deg90, false);
        }
        assert_eq!(page, sample());
    }

    #[test]
    fn invert_flips_rgb_and_keeps_alpha() {
        let page = RenderedPage {
            width: 1,
            height: 1,
            rgba: vec![10, 20, 30, 200],
        };
        let out = transform(&page, Rotation::Deg0, true);
        assert_eq!(out.rgba, [245, 235, 225, 200]);
    }

    #[test]
    fn identity_transform_copies() {
        assert_eq!(transform(&sample(), Rotation::Deg0, false), sample());
    }
}
