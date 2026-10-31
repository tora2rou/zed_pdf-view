//! 文書の書き出し。`tuzuri export`(ウィンドウを出さないコマンド)と、アプリの中の書き出しの両方で使う。

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{Diagnostic, Failure, Level, compile_file, pdf_path_for};

/// 書き出す形式。今はPDFだけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pdf,
}

impl Format {
    fn parse(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "pdf" => Ok(Self::Pdf),
            other => Err(format!("対応していない形式です: {other} (使えるのは pdf)")),
        }
    }
}

/// `tuzuri export`の引数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArgs {
    pub input: PathBuf,
    /// 省略したら、入力と同じフォルダに拡張子を変えて書く。
    pub output: Option<PathBuf>,
    pub format: Format,
}

impl ExportArgs {
    /// 書き出し先。
    pub fn output_path(&self) -> PathBuf {
        match (&self.output, self.format) {
            (Some(output), _) => output.clone(),
            (None, Format::Pdf) => pdf_path_for(&self.input),
        }
    }
}

pub const EXPORT_USAGE: &str = "\
使い方: tuzuri export <入力.typ> [-o <出力>] [--format pdf]

  -o, --output <出力>   書き出すファイル (省略すると、入力の拡張子を .pdf にしたもの)
  -f, --format <形式>   書き出す形式 (今は pdf だけ)
  -h, --help            この説明を出す";

/// `tuzuri export`の後ろの引数を読む。`Ok(None)`は`--help`。
pub fn parse_export_args(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<ExportArgs>, String> {
    let mut input = None;
    let mut output = None;
    let mut format = Format::Pdf;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} の後ろに値がありません"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--output" => output = Some(PathBuf::from(value(&arg)?)),
            "-f" | "--format" => format = Format::parse(&value(&arg)?)?,
            _ if arg.starts_with("--output=") => {
                output = Some(PathBuf::from(&arg["--output=".len()..]))
            }
            _ if arg.starts_with("--format=") => format = Format::parse(&arg["--format=".len()..])?,
            _ if arg.starts_with('-') && arg != "-" => {
                return Err(format!("知らないオプションです: {arg}"));
            }
            _ if input.is_none() => input = Some(PathBuf::from(arg)),
            _ => return Err(format!("入力のファイルは1つだけ指定してください: {arg}")),
        }
    }
    let input = input.ok_or("入力のファイルを指定してください")?;
    Ok(Some(ExportArgs {
        input,
        output,
        format,
    }))
}

/// 書き出しに成功した結果。
#[derive(Debug)]
pub struct Exported {
    pub output: PathBuf,
    pub warnings: Vec<Diagnostic>,
}

/// `args`のとおりに書き出す。
pub fn export(args: &ExportArgs) -> Result<Exported, Failure> {
    let output = args.output_path();
    match args.format {
        Format::Pdf => {
            let compiled = compile_file(&args.input)?;
            write_atomically(&output, &compiled.pdf).map_err(|error| Failure {
                errors: vec![Diagnostic::error(format!(
                    "{} に書き込めません: {error}",
                    output.display()
                ))],
                warnings: compiled.warnings.clone(),
                dependencies: compiled.dependencies.clone(),
            })?;
            Ok(Exported {
                output,
                warnings: compiled.warnings,
            })
        }
    }
}

/// `path`に`bytes`を書く。同じフォルダの一時ファイルに書いてから名前を変えるので、失敗しても
/// 書きかけのファイルは残らず、前の内容のまま(ビューアで開いていても壊れたものを見せない)。
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ファイル名がありません"))?;
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = path.with_file_name(temp_name);
    let result = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        fs::remove_file(&temp).ok();
    }
    result
}

/// エラー・警告を、端末に出す形にする (`file:line:column: error: message` とヒント)。
pub fn format_diagnostic(diagnostic: &Diagnostic) -> String {
    let level = match diagnostic.severity {
        Level::Error => "error",
        Level::Warning => "warning",
    };
    let mut out = String::new();
    if let Some(file) = &diagnostic.file {
        out.push_str(file);
        if let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) {
            out.push_str(&format!(":{line}:{column}"));
        }
        out.push_str(": ");
    }
    out.push_str(&format!("{level}: {}", diagnostic.message));
    for hint in &diagnostic.hints {
        out.push_str(&format!("\n  hint: {hint}"));
    }
    out
}

/// `tuzuri export ...`を実行する。`args`は`export`の後ろの引数。終了コードを返す
/// (成功は0、文書のエラーは1、引数の誤りは2)。
pub fn run_export_command(args: impl IntoIterator<Item = String>) -> i32 {
    let args = match parse_export_args(args) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{EXPORT_USAGE}");
            return 0;
        }
        Err(message) => {
            eprintln!("{message}\n\n{EXPORT_USAGE}");
            return 2;
        }
    };
    match export(&args) {
        Ok(exported) => {
            for warning in &exported.warnings {
                eprintln!("{}", format_diagnostic(warning));
            }
            println!("{}", exported.output.display());
            0
        }
        Err(failure) => {
            for diagnostic in failure.errors.iter().chain(&failure.warnings) {
                eprintln!("{}", format_diagnostic(diagnostic));
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<ExportArgs>, String> {
        parse_export_args(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn arguments_are_parsed() {
        let args = parse(&["doc.typ"]).unwrap().unwrap();
        assert_eq!(args.input, Path::new("doc.typ"));
        assert_eq!(args.output_path(), Path::new("doc.pdf"));

        for options in [
            &["doc.typ", "-o", "out.pdf"][..],
            &["--output", "out.pdf", "doc.typ"],
            &["doc.typ", "--output=out.pdf", "--format=PDF"],
            &["-f", "pdf", "doc.typ", "-o", "out.pdf"],
        ] {
            let args = parse(options).unwrap().unwrap();
            assert_eq!(args.output_path(), Path::new("out.pdf"), "{options:?}");
            assert_eq!(args.format, Format::Pdf);
        }
        assert_eq!(parse(&["--help"]).unwrap(), None);
    }

    #[test]
    fn bad_arguments_are_explained() {
        assert!(parse(&[]).unwrap_err().contains("入力"));
        assert!(parse(&["a.typ", "b.typ"]).unwrap_err().contains("1つだけ"));
        assert!(
            parse(&["a.typ", "-o"])
                .unwrap_err()
                .contains("値がありません")
        );
        assert!(
            parse(&["a.typ", "--format", "html"])
                .unwrap_err()
                .contains("html")
        );
        assert!(
            parse(&["a.typ", "--verbose"])
                .unwrap_err()
                .contains("--verbose")
        );
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("typst_export_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_document_is_exported_next_to_the_source() {
        let dir = temp_dir("next_to");
        fs::write(dir.join("doc.typ"), "= Hello").unwrap();
        let args = parse(&[dir.join("doc.typ").to_str().unwrap()])
            .unwrap()
            .unwrap();
        let exported = export(&args).unwrap();
        assert_eq!(exported.output, dir.join("doc.pdf"));
        assert!(fs::read(&exported.output).unwrap().starts_with(b"%PDF-"));
    }

    #[test]
    fn a_broken_document_keeps_the_previous_output() {
        let dir = temp_dir("broken");
        let input = dir.join("doc.typ");
        let output = dir.join("out.pdf");
        fs::write(&output, b"previous").unwrap();
        fs::write(&input, "#unknown()").unwrap();
        let args = ExportArgs {
            input,
            output: Some(output.clone()),
            format: Format::Pdf,
        };
        let failure = export(&args).unwrap_err();
        assert_eq!(failure.errors[0].line, Some(1));
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }

    #[test]
    fn writing_replaces_the_file_without_leaving_temporaries() {
        let dir = temp_dir("atomic");
        let path = dir.join("out.pdf");
        write_atomically(&path, b"one").unwrap();
        write_atomically(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["out.pdf"]);
        // 書けない場所なら、エラーになり一時ファイルも残らない。
        assert!(write_atomically(&dir.join("missing/out.pdf"), b"x").is_err());
    }

    #[test]
    fn diagnostics_are_printed_with_their_location_and_hints() {
        let diagnostic = Diagnostic {
            file: Some("main.typ".to_string()),
            line: Some(4),
            column: Some(2),
            hints: vec!["did you mean `text`?".to_string()],
            ..Diagnostic::error("unknown variable: txt".to_string())
        };
        assert_eq!(
            format_diagnostic(&diagnostic),
            "main.typ:4:2: error: unknown variable: txt\n  hint: did you mean `text`?"
        );
    }
}
