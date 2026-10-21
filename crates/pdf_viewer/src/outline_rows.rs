//! 目次を、画面に並べる行のリストにする。GPUIに依存しない純粋関数。

use std::collections::HashSet;

use pdf_core::OutlineItem;

/// 目次の項目を指す経路(各階層での位置)。例: 2番目の項目の1番目の子 = `[1, 0]`。
pub type OutlinePath = Vec<usize>;

/// 画面に並べる1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub path: OutlinePath,
    pub title: String,
    pub page: Option<usize>,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
}

/// `expanded`に含まれる項目だけ子を開いた状態で、目次を行に並べる。
pub fn flatten(items: &[OutlineItem], expanded: &HashSet<OutlinePath>) -> Vec<OutlineRow> {
    let mut rows = Vec::new();
    push_rows(items, expanded, &mut Vec::new(), &mut rows);
    rows
}

fn push_rows(
    items: &[OutlineItem],
    expanded: &HashSet<OutlinePath>,
    path: &mut OutlinePath,
    rows: &mut Vec<OutlineRow>,
) {
    for (index, item) in items.iter().enumerate() {
        path.push(index);
        let is_expanded = expanded.contains(path);
        rows.push(OutlineRow {
            path: path.clone(),
            title: item.title.clone(),
            page: item.page,
            depth: path.len() - 1,
            has_children: !item.children.is_empty(),
            expanded: is_expanded,
        });
        if is_expanded {
            push_rows(&item.children, expanded, path, rows);
        }
        path.pop();
    }
}

/// `path`の開閉を切り替える。
pub fn toggle(expanded: &mut HashSet<OutlinePath>, path: &OutlinePath) {
    if !expanded.remove(path) {
        expanded.insert(path.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, page: Option<usize>, children: Vec<OutlineItem>) -> OutlineItem {
        OutlineItem {
            title: title.into(),
            page,
            children,
        }
    }

    fn sample() -> Vec<OutlineItem> {
        vec![
            item(
                "A",
                Some(0),
                vec![
                    item("A1", Some(1), vec![item("A1a", None, vec![])]),
                    item("A2", Some(2), vec![]),
                ],
            ),
            item("B", Some(3), vec![]),
        ]
    }

    fn titles(rows: &[OutlineRow]) -> Vec<&str> {
        rows.iter().map(|r| r.title.as_str()).collect()
    }

    #[test]
    fn collapsed_shows_only_top_level() {
        let rows = flatten(&sample(), &HashSet::new());
        assert_eq!(titles(&rows), ["A", "B"]);
        assert!(rows[0].has_children && !rows[0].expanded);
        assert!(!rows[1].has_children);
    }

    #[test]
    fn expanding_shows_children_in_order_with_depth() {
        let expanded = HashSet::from([vec![0], vec![0, 0]]);
        let rows = flatten(&sample(), &expanded);
        assert_eq!(titles(&rows), ["A", "A1", "A1a", "A2", "B"]);
        assert_eq!(
            rows.iter().map(|r| r.depth).collect::<Vec<_>>(),
            [0, 1, 2, 1, 0]
        );
        assert_eq!(rows[2].path, [0, 0, 0]);
    }

    #[test]
    fn a_child_stays_hidden_when_only_the_grandparent_is_expanded() {
        // [0,0] だけ開いていても、親の[0]が閉じていれば子は出ない
        let rows = flatten(&sample(), &HashSet::from([vec![0, 0]]));
        assert_eq!(titles(&rows), ["A", "B"]);
    }

    #[test]
    fn toggle_flips_the_state() {
        let mut expanded = HashSet::new();
        toggle(&mut expanded, &vec![0]);
        assert!(expanded.contains(&vec![0]));
        toggle(&mut expanded, &vec![0]);
        assert!(expanded.is_empty());
    }

    #[test]
    fn empty_outline_has_no_rows() {
        assert!(flatten(&[], &HashSet::new()).is_empty());
    }
}
