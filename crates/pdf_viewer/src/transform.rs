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

    /// 回転後の画像上の点`(x, y)`を、回転前の画像上の点に戻す。
    /// `base_size`は回転前の画像の大きさ(幅, 高さ)。連続値の座標で、画素の中心の補正はしない。
    pub fn to_base(self, (width, height): (f32, f32), (x, y): (f32, f32)) -> (f32, f32) {
        match self {
            Self::Deg0 => (x, y),
            Self::Deg90 => (y, height - x),
            Self::Deg180 => (width - x, height - y),
            Self::Deg270 => (width - y, x),
        }
    }

    /// 回転前の画像上の点を、回転後の画像上の点にする(`to_base`の逆)。テストで往復を確かめるのに使う。
    #[cfg(test)]
    pub(crate) fn rotate_point(
        self,
        (width, height): (f32, f32),
        (x, y): (f32, f32),
    ) -> (f32, f32) {
        match self {
            Self::Deg0 => (x, y),
            Self::Deg90 => (height - y, x),
            Self::Deg180 => (width - x, height - y),
            Self::Deg270 => (y, width - x),
        }
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

/// ページの見た目の設定(回転と白黒反転)。これが変わると画像を作り直す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Appearance {
    pub rotation: Rotation,
    pub invert: bool,
}

/// 回転・反転した新しい画像を作る。何も変えないときもコピーを返す。
pub fn transform(page: &RenderedPage, appearance: Appearance) -> RenderedPage {
    let mut out = rotate(page, appearance.rotation);
    if appearance.invert {
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

    fn rot(rotation: Rotation) -> Appearance {
        Appearance {
            rotation,
            invert: false,
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
    fn points_round_trip_through_every_rotation() {
        let base = (200.0, 100.0);
        for r in [
            Rotation::Deg0,
            Rotation::Deg90,
            Rotation::Deg180,
            Rotation::Deg270,
        ] {
            for p in [(0.0, 0.0), (10.0, 70.0), (200.0, 100.0), (123.5, 45.25)] {
                assert_eq!(r.to_base(base, r.rotate_point(base, p)), p, "{r:?} {p:?}");
            }
        }
    }

    #[test]
    fn rotating_a_point_matches_rotating_the_image() {
        // 200x100の画像の左上(0,0)の画素は、時計回り90度で右上(100,0)になる
        let base = (200.0, 100.0);
        assert_eq!(Rotation::Deg90.rotate_point(base, (0.0, 0.0)), (100.0, 0.0));
        // 左下(0,100)は左上(0,0)へ
        assert_eq!(Rotation::Deg90.rotate_point(base, (0.0, 100.0)), (0.0, 0.0));
        // 270度: 左上(0,0)は左下(0,200)へ
        assert_eq!(
            Rotation::Deg270.rotate_point(base, (0.0, 0.0)),
            (0.0, 200.0)
        );
        // 180度: 左上は右下へ
        assert_eq!(
            Rotation::Deg180.rotate_point(base, (0.0, 0.0)),
            (200.0, 100.0)
        );
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
        let out = transform(&sample(), rot(Rotation::Deg90));
        assert_eq!((out.width, out.height), (1, 2));
        assert_eq!(out.rgba, [RED, BLUE].concat());
    }

    #[test]
    fn rotate_180_reverses_order() {
        let out = transform(&sample(), rot(Rotation::Deg180));
        assert_eq!((out.width, out.height), (2, 1));
        assert_eq!(out.rgba, [BLUE, RED].concat());
    }

    #[test]
    fn rotate_270_puts_left_pixel_on_bottom() {
        let out = transform(&sample(), rot(Rotation::Deg270));
        assert_eq!((out.width, out.height), (1, 2));
        assert_eq!(out.rgba, [BLUE, RED].concat());
    }

    #[test]
    fn four_quarter_turns_restore_the_image() {
        let mut page = sample();
        for _ in 0..4 {
            page = transform(&page, rot(Rotation::Deg90));
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
        let out = transform(
            &page,
            Appearance {
                invert: true,
                ..Default::default()
            },
        );
        assert_eq!(out.rgba, [245, 235, 225, 200]);
    }

    #[test]
    fn identity_transform_copies() {
        assert_eq!(transform(&sample(), Appearance::default()), sample());
    }
}
