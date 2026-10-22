//! LaTeXのログ(`-file-line-error`つき)から、エラーと警告を取り出す。

use std::path::{Path, PathBuf};

use build_diag::{Diagnostic, Level};

/// 警告の行から、「on input line N」の行番号を探す行数。警告は数行に折り返される。
const WARNING_LOOKAHEAD: usize = 4;
/// エラーの行から、文脈の行(`l.N ...`)を探す行数。
const CONTEXT_LOOKAHEAD: usize = 8;

/// ログを読んで、エラーと警告を返す。`base`はコンパイルしたフォルダ(`./x.tex`の起点)、`main`は文書本体。
pub fn parse(log: &str, base: &Path, main: &Path) -> Vec<Diagnostic> {
    let lines: Vec<&str> = log.lines().collect();
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let found = if let Some((file, number, message)) = file_line_error(line) {
            Some(error_at(file, number, message, &lines[i + 1..], base))
        } else if let Some(message) = line.strip_prefix("! ") {
            bare_error(message, &lines[i + 1..], main, base)
        } else {
            warning(line, &lines[i + 1..], main, base)
        };
        if let Some(diagnostic) = found
            && out.last() != Some(&diagnostic)
        {
            out.push(diagnostic);
        }
        i += 1;
    }
    out
}

/// `./main.tex:12: message`の形の行を、ファイル・行番号・メッセージに分ける。
fn file_line_error(line: &str) -> Option<(&str, usize, &str)> {
    for (at, _) in line.match_indices(':') {
        let rest = &line[at + 1..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || !rest[digits..].starts_with(": ") {
            continue;
        }
        let file = &line[..at];
        if looks_like_a_file(file) {
            return Some((
                file,
                rest[..digits].parse().ok()?,
                rest[digits + 2..].trim(),
            ));
        }
    }
    None
}

/// 拡張子が付いた名前か(`Package foo Warning`のような文章を、ファイル名と取り違えない)。
fn looks_like_a_file(text: &str) -> bool {
    let name = text.rsplit(['/', '\\']).next().unwrap_or(text);
    match name.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && (1..=4).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && !text.contains(' ')
                || text.starts_with("./") && (1..=4).contains(&ext.len())
        }
        None => false,
    }
}

fn resolve(file: &str, base: &Path) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(file.strip_prefix("./").unwrap_or(file))
    }
}

/// 表示用の名前。コンパイルしたフォルダの中なら、そこからの相対パス。
fn display(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// 直後の行にある`l.12 \foo`の文脈の行(と、その続き)。
fn context(following: &[&str]) -> Option<String> {
    let (at, line) = following
        .iter()
        .take(CONTEXT_LOOKAHEAD)
        .enumerate()
        .find(|(_, l)| l.starts_with("l.") && l[2..].starts_with(|c: char| c.is_ascii_digit()))?;
    let rest = following.get(at + 1).map(|l| l.trim()).unwrap_or("");
    Some(format!("{} {}", line.trim(), rest).trim_end().to_string())
}

fn error_at(
    file: &str,
    number: usize,
    message: &str,
    following: &[&str],
    base: &Path,
) -> Diagnostic {
    let path = resolve(file, base);
    let mut hints = Vec::new();
    hints.extend(context(following));
    Diagnostic {
        severity: Level::Error,
        message: message.to_string(),
        file: Some(display(&path, base)),
        path: Some(path),
        line: Some(number),
        column: None,
        hints,
    }
}

/// 場所のつかない`! message`の形のエラー。直後の`l.N`から行を推定する。後始末の行は無視する。
fn bare_error(message: &str, following: &[&str], main: &Path, base: &Path) -> Option<Diagnostic> {
    let message = message.trim();
    if message.starts_with("Emergency stop")
        || message.starts_with("==> Fatal")
        || message.is_empty()
    {
        return None;
    }
    let hint = context(following);
    let line = hint
        .as_deref()
        .and_then(|h| h[2..].split_whitespace().next())
        .and_then(|n| n.parse().ok());
    Some(Diagnostic {
        severity: Level::Error,
        message: message.to_string(),
        file: Some(display(main, base)),
        path: Some(main.to_path_buf()),
        line,
        column: None,
        hints: hint.into_iter().collect(),
    })
}

fn warning(line: &str, following: &[&str], main: &Path, base: &Path) -> Option<Diagnostic> {
    let message = if let Some(rest) = line.strip_prefix("LaTeX Warning: ") {
        rest
    } else if line.starts_with("Package ") || line.starts_with("Class ") {
        let (_, rest) = line.split_once(" Warning: ")?;
        rest
    } else {
        return None;
    };
    // 組み直しで直るもの(参照の未確定など)は、何度かの実行の途中の状態なので、取り上げない。
    if message.contains("Rerun to get") || message.contains("may have changed") {
        return None;
    }
    let number = std::iter::once(line)
        .chain(following.iter().copied().take(WARNING_LOOKAHEAD))
        .find_map(|l| {
            let at = l.find("on input line ")? + "on input line ".len();
            l[at..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        });
    Some(Diagnostic {
        severity: Level::Warning,
        message: message.trim().trim_end_matches('.').to_string(),
        file: Some(display(main, base)),
        path: Some(main.to_path_buf()),
        line: number,
        column: None,
        hints: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "/work";

    fn run(log: &str) -> Vec<Diagnostic> {
        parse(log, Path::new(BASE), Path::new("/work/main.tex"))
    }

    #[test]
    fn a_file_line_error_has_its_location_and_context() {
        let log = "./main.tex:4: Undefined control sequence.\nl.4 Hello \\foo\n               world.\nThe control sequence at the end\n";
        let d = &run(log)[0];
        assert_eq!(d.severity, Level::Error);
        assert_eq!(d.message, "Undefined control sequence.");
        assert_eq!(d.file.as_deref(), Some("main.tex"));
        assert_eq!(d.path.as_deref(), Some(Path::new("/work/main.tex")));
        assert_eq!(d.line, Some(4));
        assert_eq!(d.hints, ["l.4 Hello \\foo world."]);
    }

    /// 読み込んだ別ファイルのエラーは、そのファイルを指す。
    #[test]
    fn errors_in_other_files_point_to_them() {
        let d = &run("./chapters/one.tex:9: Missing $ inserted.\n")[0];
        assert_eq!(d.file.as_deref(), Some("chapters/one.tex"));
        assert_eq!(d.path.as_deref(), Some(Path::new("/work/chapters/one.tex")));
    }

    #[test]
    fn absolute_paths_stay_absolute() {
        let d = &run("/usr/share/texlive/foo.sty:10: Package foo Error: bad.\n")[0];
        assert_eq!(d.file.as_deref(), Some("/usr/share/texlive/foo.sty"));
    }

    #[test]
    fn a_bare_error_takes_its_line_from_the_context() {
        let log = "! File ended while scanning use of \\frac .\n<inserted text> \n  \\par \nl.9 \\end{equation}\n";
        let d = &run(log)[0];
        assert_eq!(d.message, "File ended while scanning use of \\frac .");
        assert_eq!(d.line, Some(9));
        assert_eq!(d.file.as_deref(), Some("main.tex"));
    }

    #[test]
    fn cleanup_noise_is_ignored() {
        let log = "! Emergency stop.\n<*> main.tex\n!  ==> Fatal error occurred, no output PDF file produced!\n";
        assert!(run(log).is_empty());
    }

    #[test]
    fn warnings_take_the_input_line() {
        let log = "LaTeX Warning: Reference `nope' on page 1 undefined on input line 6.\n";
        let d = &run(log)[0];
        assert_eq!(d.severity, Level::Warning);
        assert_eq!(
            d.message,
            "Reference `nope' on page 1 undefined on input line 6"
        );
        assert_eq!(d.line, Some(6));
    }

    #[test]
    fn package_warnings_wrap_across_lines() {
        let log = "Package hyperref Warning: Token not allowed in a PDF string (PDFDocEncoding):\n(hyperref)                removing `math shift' on input line 12.\n";
        let d = &run(log)[0];
        assert_eq!(d.severity, Level::Warning);
        assert_eq!(d.line, Some(12));
    }

    /// 組み直しで直る警告は、途中の状態なので出さない。
    #[test]
    fn rerun_warnings_are_skipped() {
        assert!(
            run("LaTeX Warning: Label(s) may have changed. Rerun to get cross-references right.\n")
                .is_empty()
        );
    }

    /// 文章の途中の`:`+数字を、ファイルと行と取り違えない。
    #[test]
    fn prose_with_colons_is_not_an_error() {
        assert!(
            run("Output written on main.pdf (1 page, 1234 bytes).\nTranscript: 3: something\n")
                .is_empty()
        );
    }

    #[test]
    fn identical_neighbours_are_merged() {
        let log = "./main.tex:4: Oops.\n./main.tex:4: Oops.\n";
        assert_eq!(run(log).len(), 1);
    }
}
