//! ドラッグでのスクロール(手のひらツール)の計算。GPUIに依存しない。

/// ドラッグ開始時のスクロール位置`start`から、マウスが`start_mouse`から`mouse`へ動いたときの
/// スクロール位置(正の値 = 内容を上/左にずらした量)。内容はマウスに付いてくるので、
/// マウスを下に動かすとスクロール位置は減る。`max`を超えず、負にもならない。
pub fn dragged_scroll(
    start: (f32, f32),
    start_mouse: (f32, f32),
    mouse: (f32, f32),
    max: (f32, f32),
) -> (f32, f32) {
    let x = (start.0 - (mouse.0 - start_mouse.0)).clamp(0.0, max.0.max(0.0));
    let y = (start.1 - (mouse.1 - start_mouse.1)).clamp(0.0, max.1.max(0.0));
    (x, y)
}

/// 範囲選択のドラッグ中、ポインタが表示領域`start..end`の外に出ているとき、1フレームあたりに
/// 自動でスクロールする量(正の値 = 下/右へ)。領域の中なら0。外に出るほど速く、上限がある。
pub fn edge_autoscroll(pointer: f32, start: f32, end: f32) -> f32 {
    const SPEED_PER_PX: f32 = 0.25;
    const MIN_STEP: f32 = 2.0;
    const MAX_STEP: f32 = 40.0;
    if pointer < start {
        -((start - pointer) * SPEED_PER_PX + MIN_STEP).min(MAX_STEP)
    } else if pointer > end {
        ((pointer - end) * SPEED_PER_PX + MIN_STEP).min(MAX_STEP)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: (f32, f32) = (500.0, 1000.0);

    #[test]
    fn content_follows_the_mouse() {
        // マウスを下・右へ動かすと、内容も下・右へ動く = スクロール位置は減る
        let out = dragged_scroll((100.0, 300.0), (50.0, 50.0), (70.0, 90.0), MAX);
        assert_eq!(out, (80.0, 260.0));
    }

    #[test]
    fn no_movement_keeps_position() {
        let out = dragged_scroll((100.0, 300.0), (50.0, 50.0), (50.0, 50.0), MAX);
        assert_eq!(out, (100.0, 300.0));
    }

    #[test]
    fn clamps_to_the_scrollable_range() {
        assert_eq!(
            dragged_scroll((10.0, 10.0), (0.0, 0.0), (100.0, 100.0), MAX),
            (0.0, 0.0)
        );
        assert_eq!(
            dragged_scroll((490.0, 990.0), (0.0, 0.0), (-100.0, -100.0), MAX),
            MAX
        );
    }

    #[test]
    fn unscrollable_axis_stays_at_zero() {
        let out = dragged_scroll((0.0, 0.0), (0.0, 0.0), (-50.0, -50.0), (0.0, 100.0));
        assert_eq!(out, (0.0, 50.0));
    }

    #[test]
    fn autoscroll_is_zero_inside_the_area() {
        assert_eq!(edge_autoscroll(100.0, 36.0, 900.0), 0.0);
        assert_eq!(edge_autoscroll(36.0, 36.0, 900.0), 0.0);
        assert_eq!(edge_autoscroll(900.0, 36.0, 900.0), 0.0);
    }

    #[test]
    fn autoscroll_points_toward_the_pointer_and_speeds_up_with_distance() {
        let near_below = edge_autoscroll(910.0, 36.0, 900.0);
        let far_below = edge_autoscroll(1000.0, 36.0, 900.0);
        assert!(near_below > 0.0 && far_below > near_below);
        let near_above = edge_autoscroll(26.0, 36.0, 900.0);
        let far_above = edge_autoscroll(-60.0, 36.0, 900.0);
        assert!(near_above < 0.0 && far_above < near_above);
    }

    #[test]
    fn autoscroll_has_a_speed_limit() {
        assert_eq!(edge_autoscroll(100_000.0, 36.0, 900.0), 40.0);
        assert_eq!(edge_autoscroll(-100_000.0, 36.0, 900.0), -40.0);
    }
}
