//! ズーム倍率の計算。GPUIに依存しない純粋関数だけを置く。

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
}
