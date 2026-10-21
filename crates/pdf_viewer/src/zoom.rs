//! ズーム倍率の計算。GPUIに依存しない純粋関数だけを置く。

use crate::layout::MARGIN;

/// 倍率の決め方。`Fit*`の間は、ウィンドウの大きさに追従して倍率を計算し直す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoomMode {
    #[default]
    Manual,
    /// 最も幅の広いページが、ビューポートの幅にちょうど収まる倍率。
    FitWidth,
    /// 現在のページが、ビューポートに丸ごと収まる倍率。
    FitPage,
}

pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 5.0;
pub const DEFAULT_ZOOM: f32 = 1.0;
/// ボタン/キーボードで1回操作したときの倍率の変化。
pub const ZOOM_STEP: f32 = 1.25;
/// ホイール1pxあたりの倍率の変化(`exp(delta * この値)`)。
pub const WHEEL_SENSITIVITY: f32 = 0.005;

/// 倍率を許容範囲に収める。NaNは既定値に戻す。
pub fn clamp(zoom: f32) -> f32 {
    if zoom.is_nan() {
        DEFAULT_ZOOM
    } else {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

/// ホイールの縦移動量(上方向が正)に対応する倍率の係数。
pub fn wheel_factor(delta_y: f32) -> f32 {
    (delta_y * WHEEL_SENSITIVITY).exp()
}

/// 倍率を`old`から`new`に変えても、ビューポート内の`anchor`の位置に見えている
/// 内容が動かないようにする新しいスクロール量(正の値 = 内容を上/左にずらした量)。
pub fn scroll_after_zoom(scroll: f32, anchor: f32, old: f32, new: f32) -> f32 {
    (scroll + anchor) * (new / old) - anchor
}

/// ページ幅`page_width`(PDFポイント)が、余白を除いたビューポート幅に収まる倍率。
pub fn fit_width(viewport_width: f32, page_width: f32) -> f32 {
    fit(viewport_width, page_width)
}

/// ページ`page_size`(PDFポイント)が、余白を除いたビューポートに丸ごと収まる倍率。
pub fn fit_page(viewport: (f32, f32), page_size: (f32, f32)) -> f32 {
    fit(viewport.0, page_size.0).min(fit(viewport.1, page_size.1))
}

fn fit(viewport: f32, page: f32) -> f32 {
    if page > 0.0 {
        clamp((viewport - MARGIN * 2.0) / page)
    } else {
        DEFAULT_ZOOM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_limits_range() {
        assert_eq!(clamp(0.01), MIN_ZOOM);
        assert_eq!(clamp(100.0), MAX_ZOOM);
        assert_eq!(clamp(2.0), 2.0);
        assert_eq!(clamp(f32::NAN), DEFAULT_ZOOM);
    }

    #[test]
    fn wheel_factor_is_symmetric() {
        assert_eq!(wheel_factor(0.0), 1.0);
        assert!(wheel_factor(10.0) > 1.0);
        assert!((wheel_factor(10.0) * wheel_factor(-10.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn anchor_point_stays_in_place() {
        // 内容座標 (scroll + anchor) / zoom が拡大前後で同じ
        let (scroll, anchor, old, new) = (300.0, 100.0, 1.0, 2.0);
        let after = scroll_after_zoom(scroll, anchor, old, new);
        assert_eq!((scroll + anchor) / old, (after + anchor) / new);
    }

    #[test]
    fn zoom_at_top_left_anchor_scales_scroll() {
        assert_eq!(scroll_after_zoom(100.0, 0.0, 1.0, 2.0), 200.0);
    }

    #[test]
    fn fit_width_leaves_margins() {
        assert_eq!(fit_width(600.0 + MARGIN * 2.0, 300.0), 2.0);
    }

    #[test]
    fn fit_page_uses_the_tighter_axis() {
        let viewport = (400.0 + MARGIN * 2.0, 400.0 + MARGIN * 2.0);
        // 縦長ページは高さで、横長ページは幅で決まる
        assert_eq!(fit_page(viewport, (100.0, 800.0)), 0.5);
        assert_eq!(fit_page(viewport, (800.0, 100.0)), 0.5);
    }

    #[test]
    fn fit_is_clamped_and_handles_empty_pages() {
        assert_eq!(fit_width(10_000.0, 10.0), MAX_ZOOM);
        assert_eq!(fit_width(1.0, 1000.0), MIN_ZOOM);
        assert_eq!(fit_width(500.0, 0.0), DEFAULT_ZOOM);
    }
}
