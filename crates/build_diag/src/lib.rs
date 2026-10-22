//! 文書のコンパイル(Typst・LaTeX)が返す、エラーと警告の共通の型。

use std::path::PathBuf;

/// エラーまたは警告1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Level,
    pub message: String,
    /// 該当するファイル。プロジェクトの外(パッケージなど)のファイルは、仮想パスのまま。
    pub file: Option<String>,
    /// 該当するファイルの、ファイルシステム上の場所(エディタで開くための絶対パス)。
    pub path: Option<PathBuf>,
    /// 該当する位置(1始まりの行・列)。
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Diagnostic {
    /// 位置のない、メッセージだけの診断。
    pub fn message(severity: Level, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
            file: None,
            path: None,
            line: None,
            column: None,
            hints: Vec::new(),
        }
    }

    /// `file:line:column: message`の1行の形。
    pub fn one_line(&self) -> String {
        let mut out = String::new();
        if let Some(file) = &self.file {
            out.push_str(file);
            if let Some(line) = self.line {
                out.push_str(&format!(":{line}"));
                if let Some(column) = self.column {
                    out.push_str(&format!(":{column}"));
                }
            }
            out.push_str(": ");
        }
        out.push_str(&self.message);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_with_a_full_location() {
        let d = Diagnostic {
            file: Some("main.typ".into()),
            line: Some(4),
            column: Some(2),
            ..Diagnostic::message(Level::Error, "boom")
        };
        assert_eq!(d.one_line(), "main.typ:4:2: boom");
    }

    /// LaTeXのエラーは列を持たない。
    #[test]
    fn one_line_without_a_column() {
        let d = Diagnostic {
            file: Some("main.tex".into()),
            line: Some(12),
            ..Diagnostic::message(Level::Error, "Undefined control sequence.")
        };
        assert_eq!(d.one_line(), "main.tex:12: Undefined control sequence.");
    }

    #[test]
    fn one_line_without_a_location() {
        assert_eq!(Diagnostic::message(Level::Warning, "w").one_line(), "w");
    }
}
