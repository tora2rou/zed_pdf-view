use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use pdf_core::RenderedPage;

/// `RenderedPage`(RGBA)をGPUIが描画できる`RenderImage`(BGRA)へ変換する。
/// ピクセル数とバッファ長が合わない場合は`None`。
pub fn to_render_image(page: &RenderedPage) -> Option<Arc<RenderImage>> {
    let mut bgra = page.rgba.clone();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let buffer = RgbaImage::from_raw(page.width, page.height, bgra)?;
    Some(Arc::new(RenderImage::new([Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_red_and_blue() {
        let page = RenderedPage {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 10, 20, 30, 255],
        };
        let image = to_render_image(&page).unwrap();
        assert_eq!(image.as_bytes(0).unwrap(), [3, 2, 1, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn keeps_size() {
        let page = RenderedPage {
            width: 3,
            height: 2,
            rgba: vec![0; 3 * 2 * 4],
        };
        let size = to_render_image(&page).unwrap().size(0);
        assert_eq!((size.width.0, size.height.0), (3, 2));
    }

    #[test]
    fn rejects_mismatched_buffer() {
        let page = RenderedPage {
            width: 3,
            height: 2,
            rgba: vec![0; 5],
        };
        assert!(to_render_image(&page).is_none());
    }
}
