//! ツールバーのページ番号入力欄の編集ロジック。GPUIに依存しない。

/// 入力できる最大桁数。
const MAX_DIGITS: usize = 6;

/// 編集中のページ番号(1始まりの数字列)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageInput {
    text: String,
}

impl PageInput {
    /// 現在のページ番号(1始まり)を初期値にして編集を始める。
    pub fn new(current_page: usize) -> Self {
        Self {
            text: current_page.to_string(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// 数字だけ受け付ける。受け付けたら`true`。
    pub fn push(&mut self, c: char) -> bool {
        if c.is_ascii_digit() && self.text.len() < MAX_DIGITS {
            self.text.push(c);
            true
        } else {
            false
        }
    }

    pub fn backspace(&mut self) {
        self.text.pop();
    }

    /// 入力を確定して、移動先のページ番号(0始まり)を返す。範囲外は端に丸める。
    /// 空や0のみなどで数として読めなければ`None`。
    pub fn commit(&self, page_count: usize) -> Option<usize> {
        let page: usize = self.text.parse().ok()?;
        if page_count == 0 {
            return None;
        }
        Some(page.clamp(1, page_count) - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_current_page() {
        assert_eq!(PageInput::new(3).text(), "3");
    }

    #[test]
    fn accepts_only_digits() {
        let mut input = PageInput::default();
        assert!(input.push('4'));
        assert!(!input.push('a'));
        assert!(!input.push('-'));
        assert!(!input.push('٣')); // ASCII以外の数字
        assert_eq!(input.text(), "4");
    }

    #[test]
    fn limits_length() {
        let mut input = PageInput::default();
        for _ in 0..10 {
            input.push('1');
        }
        assert_eq!(input.text().len(), MAX_DIGITS);
    }

    #[test]
    fn backspace_removes_last_digit_and_tolerates_empty() {
        let mut input = PageInput::new(12);
        input.backspace();
        assert_eq!(input.text(), "1");
        input.backspace();
        input.backspace();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn commit_converts_to_zero_based_and_clamps() {
        assert_eq!(PageInput::new(3).commit(10), Some(2));
        assert_eq!(PageInput::new(99).commit(10), Some(9));
        assert_eq!(PageInput::new(0).commit(10), Some(0));
    }

    #[test]
    fn commit_rejects_empty_input_and_empty_document() {
        let mut input = PageInput::new(1);
        input.backspace();
        assert_eq!(input.commit(10), None);
        assert_eq!(PageInput::new(1).commit(0), None);
    }
}
