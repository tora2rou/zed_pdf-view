use std::{collections::HashMap, ops::Range, sync::Arc};

use gpui::{RenderImage, Window};
use pdf_core::{PdfDocument, PdfError};

use crate::{
    to_render_image,
    transform::{Appearance, transform},
};

/// 画像を作り直す必要があるかを決める条件(描画倍率と見た目の設定)。
#[derive(PartialEq, Eq)]
struct ImageKey {
    scale_bits: u32,
    appearance: Appearance,
}

/// ページ画像1枚。`key`が変わったら作り直す。
struct PageImage {
    key: ImageKey,
    image: Arc<RenderImage>,
}

/// 表示中のページのGPU用画像を保持する。画面外に出たページの画像は破棄する。
#[derive(Default)]
pub struct PageImages {
    images: HashMap<usize, PageImage>,
}

impl PageImages {
    /// ページ`index`を`scale`倍で描画した画像を返す。倍率が同じなら保持している画像を使い回す。
    pub fn get_or_render(
        &mut self,
        document: &PdfDocument,
        index: usize,
        scale: f32,
        appearance: Appearance,
        window: &mut Window,
    ) -> Result<Arc<RenderImage>, PdfError> {
        let key = ImageKey {
            scale_bits: scale.to_bits(),
            appearance,
        };
        if let Some(cached) = self.images.get(&index).filter(|c| c.key == key) {
            return Ok(cached.image.clone());
        }
        let rendered = document.render_page(index, scale)?;
        let image = to_render_image(&transform(&rendered, appearance))
            .ok_or_else(|| PdfError::Load("画像データのサイズが不正です".into()))?;
        let new = PageImage {
            key,
            image: image.clone(),
        };
        if let Some(old) = self.images.insert(index, new) {
            drop_from_gpu(old.image, window);
        }
        Ok(image)
    }

    /// `visible`の範囲外のページ画像を破棄する。
    pub fn retain_visible(&mut self, visible: &Range<usize>, window: &mut Window) {
        let hidden: Vec<usize> = self
            .images
            .keys()
            .copied()
            .filter(|index| !visible.contains(index))
            .collect();
        for index in hidden {
            if let Some(old) = self.images.remove(&index) {
                drop_from_gpu(old.image, window);
            }
        }
    }
}

/// 古い画像がGPUのアトラスに残り続けないようにする。
fn drop_from_gpu(image: Arc<RenderImage>, window: &mut Window) {
    window.drop_image(image).ok();
}
