use hayro::vello_cpu::{Pixmap, peniko::ImageAlphaType};

/// ラスタライズ済みのページ。`rgba`は白背景で塗られた不透明なRGBA8(行優先、隙間なし)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RenderedPage {
    /// 不透明な背景で描画済みのPixmapから作る。
    /// 背景が不透明なのでpremultipliedでもstraightでも値は同じ。
    /// Pixmapのバッファをそのまま受け取るので、画素データはコピーしない。
    pub(crate) fn from_pixmap(pixmap: Pixmap) -> Self {
        let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        Self {
            width,
            height,
            rgba: pixmap.take_rgba8(ImageAlphaType::AlphaPremultiplied),
        }
    }
}
