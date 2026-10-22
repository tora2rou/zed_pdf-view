//! スクロールバーのつまみの位置と大きさの計算。GPUIに依存しない。

/// つまみの最小の長さ。内容が長くても、つまみが掴めないほど小さくならないようにする。
const MIN_THUMB: f32 = 24.0;

/// つまみの位置(トラック上端からの距離)と長さ。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thumb {
    pub top: f32,
    pub len: f32,
}

/// 長さ`track`のトラックに置くつまみ。`viewport`は見えている範囲、`content`は内容全体の長さ、
/// `offset`はスクロール位置(正の値 = 内容を上にずらした量)。スクロールする余地がなければ`None`。
pub fn thumb(track: f32, viewport: f32, content: f32, offset: f32) -> Option<Thumb> {
    let max_offset = content - viewport;
    if track <= 0.0 || viewport <= 0.0 || max_offset <= 0.0 {
        return None;
    }
    let len = (track * viewport / content).clamp(MIN_THUMB.min(track), track);
    let ratio = (offset / max_offset).clamp(0.0, 1.0);
    Some(Thumb {
        top: (track - len) * ratio,
        len,
    })
}

/// つまみを`delta`だけドラッグしたときの、スクロール位置。`start`はドラッグ開始時のスクロール位置。
/// 結果は`0..=content - viewport`に収まる。
pub fn dragged_offset(track: f32, viewport: f32, content: f32, start: f32, delta: f32) -> f32 {
    let max_offset = (content - viewport).max(0.0);
    let Some(thumb) = thumb(track, viewport, content, start) else {
        return 0.0;
    };
    let movable = track - thumb.len;
    if movable <= 0.0 {
        return start.clamp(0.0, max_offset);
    }
    (start + delta * max_offset / movable).clamp(0.0, max_offset)
}

/// トラック上の`position`(上端からの距離)をクリックしたときの、スクロール位置。
/// つまみの中心がクリックした位置に来るようにする。
pub fn offset_at(track: f32, viewport: f32, content: f32, position: f32) -> f32 {
    let max_offset = (content - viewport).max(0.0);
    let Some(thumb) = thumb(track, viewport, content, 0.0) else {
        return 0.0;
    };
    let movable = track - thumb.len;
    if movable <= 0.0 {
        return 0.0;
    }
    ((position - thumb.len / 2.0) / movable * max_offset).clamp(0.0, max_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_thumb_when_everything_fits() {
        assert_eq!(thumb(500.0, 500.0, 500.0, 0.0), None);
        assert_eq!(thumb(500.0, 500.0, 300.0, 0.0), None);
        assert_eq!(thumb(0.0, 500.0, 1000.0, 0.0), None);
    }

    #[test]
    fn thumb_length_is_proportional_to_the_visible_share() {
        let t = thumb(500.0, 500.0, 1000.0, 0.0).unwrap();
        assert_eq!(t, Thumb { top: 0.0, len: 250.0 });
    }

    #[test]
    fn thumb_reaches_the_ends_of_the_track() {
        let end = thumb(500.0, 500.0, 1000.0, 500.0).unwrap();
        assert_eq!(end.top + end.len, 500.0);
        // 範囲外のスクロール位置でも、トラックからはみ出さない。
        let over = thumb(500.0, 500.0, 1000.0, 9999.0).unwrap();
        assert_eq!(over.top + over.len, 500.0);
        assert_eq!(thumb(500.0, 500.0, 1000.0, -50.0).unwrap().top, 0.0);
    }

    #[test]
    fn thumb_has_a_minimum_length() {
        let t = thumb(500.0, 500.0, 1_000_000.0, 0.0).unwrap();
        assert_eq!(t.len, MIN_THUMB);
    }

    #[test]
    fn dragging_the_thumb_to_the_end_scrolls_to_the_end() {
        // つまみ250pxを250px動かすと、最後までスクロールする
        assert_eq!(dragged_offset(500.0, 500.0, 1000.0, 0.0, 250.0), 500.0);
        assert_eq!(dragged_offset(500.0, 500.0, 1000.0, 0.0, 9999.0), 500.0);
        assert_eq!(dragged_offset(500.0, 500.0, 1000.0, 500.0, -9999.0), 0.0);
    }

    #[test]
    fn dragging_moves_proportionally() {
        assert_eq!(dragged_offset(500.0, 500.0, 1000.0, 100.0, 50.0), 200.0);
    }

    #[test]
    fn clicking_the_track_centers_the_thumb_there() {
        // つまみ250px。中心を250pxに置くと先頭、375pxで中間、末尾側なら最後
        assert_eq!(offset_at(500.0, 500.0, 1000.0, 125.0), 0.0);
        assert_eq!(offset_at(500.0, 500.0, 1000.0, 250.0), 250.0);
        assert_eq!(offset_at(500.0, 500.0, 1000.0, 500.0), 500.0);
    }
}
