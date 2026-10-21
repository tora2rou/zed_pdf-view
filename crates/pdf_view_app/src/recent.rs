//! 最近開いたファイルの一覧。1行1パスのテキストファイルに保存する。

use std::{
    fs,
    path::{Path, PathBuf},
};

/// 保持する最大件数。
const MAX_RECENT: usize = 10;

/// 新しい順の一覧。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecentFiles {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// `path`を先頭に加える。既にあれば先頭へ移し、上限を超えた古いものは捨てる。
    pub fn add(&mut self, path: PathBuf) {
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT);
    }

    /// 1行1パスのテキストから読み込む。空行は無視する。
    pub fn parse(text: &str) -> Self {
        let mut paths: Vec<PathBuf> = Vec::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = PathBuf::from(line);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths.truncate(MAX_RECENT);
        Self { paths }
    }

    pub fn to_text(&self) -> String {
        self.paths
            .iter()
            .map(|p| format!("{}\n", p.display()))
            .collect()
    }

    /// ファイルから読み込む。無い・読めない場合は空の一覧。
    pub fn load(file: &Path) -> Self {
        fs::read_to_string(file)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// ファイルに保存する。保存できなくてもビューアの動作には影響しないので、結果だけ返す。
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(file, self.to_text())
    }
}

/// 一覧の保存先。設定ディレクトリが分からない環境では`None`。
pub fn recent_files_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(xdg)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config")
    };
    Some(base.join("pdf_view_app").join("recent.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn newest_first() {
        let mut r = RecentFiles::default();
        r.add(p("a.pdf"));
        r.add(p("b.pdf"));
        assert_eq!(r.paths(), [p("b.pdf"), p("a.pdf")]);
    }

    #[test]
    fn re_adding_moves_to_front_without_duplicates() {
        let mut r = RecentFiles::default();
        for f in ["a.pdf", "b.pdf", "c.pdf", "a.pdf"] {
            r.add(p(f));
        }
        assert_eq!(r.paths(), [p("a.pdf"), p("c.pdf"), p("b.pdf")]);
    }

    #[test]
    fn drops_the_oldest_beyond_the_limit() {
        let mut r = RecentFiles::default();
        for i in 0..(MAX_RECENT + 3) {
            r.add(p(&format!("{i}.pdf")));
        }
        assert_eq!(r.paths().len(), MAX_RECENT);
        assert_eq!(r.paths()[0], p(&format!("{}.pdf", MAX_RECENT + 2)));
        assert!(!r.paths().contains(&p("0.pdf")));
    }

    #[test]
    fn text_round_trip() {
        let mut r = RecentFiles::default();
        r.add(p("/x/a.pdf"));
        r.add(p("/y/b b.pdf"));
        assert_eq!(RecentFiles::parse(&r.to_text()), r);
    }

    #[test]
    fn parse_ignores_blank_lines_and_duplicates() {
        let r = RecentFiles::parse("\n/a.pdf\n  \n/b.pdf\n/a.pdf\n");
        assert_eq!(r.paths(), [p("/a.pdf"), p("/b.pdf")]);
    }

    #[test]
    fn load_missing_file_is_empty() {
        assert_eq!(
            RecentFiles::load(&p("/nonexistent/recent.txt")),
            RecentFiles::default()
        );
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("pdf_view_app_recent_{}", std::process::id()));
        let file = dir.join("nested").join("recent.txt");
        let mut r = RecentFiles::default();
        r.add(p("/a.pdf"));
        r.save(&file).unwrap();
        assert_eq!(RecentFiles::load(&file), r);
        fs::remove_dir_all(dir).ok();
    }
}
