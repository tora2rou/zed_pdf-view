use std::path::PathBuf;

const USAGE: &str = "使い方: pdf_view_app <file.pdf>";

/// コマンドライン引数(プログラム名を除く)から、開くPDFのパスを取り出す。
fn pdf_path_from_args(mut args: impl Iterator<Item = String>) -> Result<PathBuf, String> {
    let path = args.next().ok_or_else(|| USAGE.to_string())?;
    if args.next().is_some() {
        return Err(USAGE.to_string());
    }
    Ok(PathBuf::from(path))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn single_argument_is_the_path() {
        assert_eq!(
            pdf_path_from_args(args(&["a.pdf"])),
            Ok(PathBuf::from("a.pdf"))
        );
    }

    #[test]
    fn no_argument_shows_usage() {
        assert_eq!(pdf_path_from_args(args(&[])), Err(USAGE.to_string()));
    }

    #[test]
    fn extra_arguments_show_usage() {
        assert_eq!(
            pdf_path_from_args(args(&["a.pdf", "b.pdf"])),
            Err(USAGE.to_string())
        );
    }
}
