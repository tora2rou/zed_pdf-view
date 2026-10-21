//! アプリ「Chihayaburu」のアイコンを描く: 赤い札の上に、生成りの取り札と、明朝体の「ち」。
//! 使い方: cargo run --release -- <フォントファイル> <出力先のフォルダ>
//! 16〜256pxのPNGを書き出す(`.ico`へのまとめは make_ico.py)。
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
};
use vello_cpu::{
    Pixmap, RenderContext,
    color::{AlphaColor, Srgb},
    kurbo::{Affine, BezPath, RoundedRect, Shape},
};

struct Pen(BezPath);
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) { self.0.move_to((x as f64, y as f64)); }
    fn line_to(&mut self, x: f32, y: f32) { self.0.line_to((x as f64, y as f64)); }
    fn quad_to(&mut self, x0: f32, y0: f32, x: f32, y: f32) { self.0.quad_to((x0 as f64, y0 as f64), (x as f64, y as f64)); }
    fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.curve_to((x0 as f64, y0 as f64), (x1 as f64, y1 as f64), (x as f64, y as f64));
    }
    fn close(&mut self) { self.0.close_path(); }
}

fn rgb(hex: u32) -> AlphaColor<Srgb> {
    AlphaColor::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
}

/// 「ち」の輪郭(1em = 1、y上向き)と、その外接矩形。
fn glyph(font: &FontRef, c: char) -> BezPath {
    let gid = font.charmap().map(c).expect("glyph");
    let outline = font.outline_glyphs().get(gid).expect("outline");
    let mut pen = Pen(BezPath::new());
    outline.draw(DrawSettings::unhinted(Size::new(1.0), LocationRef::default()), &mut pen).unwrap();
    pen.0
}

fn render(size: u16, ch: &BezPath) -> Pixmap {
    let s = size as f64 / 256.0; // 256基準で描いて縮める
    let mut ctx = RenderContext::new(size, size);
    let base = Affine::scale(s);
    // 札の大きさ(実物のかるた 52x73mm の比率)
    let (w, h) = (148.0, 206.0);
    let card = RoundedRect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0, 14.0);
    // 後ろの赤い札(右に傾ける)
    let back = base * Affine::translate((150.0, 128.0)) * Affine::rotate(0.22);
    ctx.set_transform(back);
    ctx.set_paint(rgb(0x7a1f1a));
    ctx.fill_path(&card.to_path(0.1));
    ctx.set_paint(rgb(0xc0392b));
    ctx.fill_path(&RoundedRect::new(-w / 2.0 + 6.0, -h / 2.0 + 6.0, w / 2.0 - 6.0, h / 2.0 - 6.0, 10.0).to_path(0.1));
    // 手前の取り札(左に少し傾ける): 深緑の縁 + 生成りの面
    let front = base * Affine::translate((108.0, 132.0)) * Affine::rotate(-0.10);
    ctx.set_transform(front);
    ctx.set_paint(rgb(0x1f4d36));
    ctx.fill_path(&card.to_path(0.1));
    ctx.set_paint(rgb(0xf7f0dc));
    ctx.fill_path(&RoundedRect::new(-w / 2.0 + 9.0, -h / 2.0 + 9.0, w / 2.0 - 9.0, h / 2.0 - 9.0, 8.0).to_path(0.1));
    // 「ち」: 札の内側いっぱいに。フォントはy上向きなので反転する
    let bbox = ch.bounding_box();
    // 文字の大きさ(256基準)。小さいアイコンでは、細い線が消えないよう大きめにして、輪郭を太らせる。
    let small = size <= 48;
    let target = if size <= 32 { 128.0 } else { 112.0 };
    let k = target / bbox.width().max(bbox.height());
    let (cx, cy) = (bbox.center().x, bbox.center().y);
    let t = front * Affine::scale_non_uniform(k, -k) * Affine::translate((-cx, -cy));
    ctx.set_transform(t);
    ctx.set_paint(rgb(0x111111));
    ctx.fill_path(ch);
    if small {
        // 輪郭を、文字の大きさの約5%の太さでなぞって太らせる(フォント座標で指定)
        let weight = match size {
            0..=16 => 0.075,
            17..=32 => 0.05,
            _ => 0.025,
        };
        ctx.set_stroke(vello_cpu::kurbo::Stroke::new(weight));
        ctx.stroke_path(ch);
    }
    ctx.flush();
    let mut pixmap = Pixmap::new(size, size);
    ctx.render(&mut pixmap, &mut vello_cpu::Resources::default());
    pixmap
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).unwrap();
    let font = FontRef::from_index(&data, 0).unwrap();
    let ch = glyph(&font, 'ち');
    for size in [16u16, 24, 32, 48, 64, 128, 256] {
        let png = render(size, &ch).into_png().unwrap();
        std::fs::write(format!("{}/icon_{size}.png", args[2]), png).unwrap();
    }
    println!("ok");
}
