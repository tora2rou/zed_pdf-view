//! どのTeXエンジンでコンパイルするかの判定。

/// TeXのエンジン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Pdflatex,
    Xelatex,
    Lualatex,
    /// 日本語のpLaTeX(DVI経由)。
    Platex,
    /// 日本語のupLaTeX(DVI経由)。
    Uplatex,
}

impl Engine {
    /// `% !TEX program = lualatex`の`lualatex`のような名前から。
    fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "pdflatex" | "latex" | "pdftex" => Some(Self::Pdflatex),
            "xelatex" | "xetex" => Some(Self::Xelatex),
            "lualatex" | "luatex" | "luajitlatex" => Some(Self::Lualatex),
            "platex" | "ptex" => Some(Self::Platex),
            "uplatex" | "uptex" => Some(Self::Uplatex),
            _ => None,
        }
    }

    /// `latexmk`に渡す引数。
    pub fn latexmk_args(self) -> Vec<String> {
        let dvi_to_pdf = ["-pdfdvi".to_string()];
        let ptex = |latex: &str| -> Vec<String> {
            dvi_to_pdf
                .iter()
                .cloned()
                .chain([
                    "-e".to_string(),
                    format!("$latex = q/{latex} %O %S/;"),
                    "-e".to_string(),
                    "$dvipdf = q/dvipdfmx %O -o %D %S/;".to_string(),
                ])
                .collect()
        };
        match self {
            Self::Pdflatex => vec!["-pdf".into()],
            Self::Xelatex => vec!["-pdfxe".into()],
            Self::Lualatex => vec!["-pdflua".into()],
            Self::Platex => ptex("platex"),
            Self::Uplatex => ptex("uplatex"),
        }
    }
}

/// ソースの先頭にある、エンジンを指定するコメント(`% !TEX program = lualatex`)。
fn magic_comment(source: &str) -> Option<Engine> {
    source.lines().take(20).find_map(|line| {
        let comment = line.trim_start().strip_prefix('%')?.trim_start();
        let rest = comment.strip_prefix('!')?.trim_start();
        let rest = rest
            .strip_prefix("TEX")
            .or_else(|| rest.strip_prefix("tex"))?;
        let rest = rest.trim_start();
        // `TS-program`(TeXShop)と`program`の両方。
        let rest = rest.strip_prefix("TS-").unwrap_or(rest);
        let value = rest
            .strip_prefix("program")?
            .trim_start()
            .strip_prefix('=')?;
        Engine::from_name(value)
    })
}

/// `\documentclass[...]{name}`のクラス名。
fn document_class(source: &str) -> Option<&str> {
    let start = source.find("\\documentclass")? + "\\documentclass".len();
    let rest = &source[start..];
    let rest = rest.trim_start();
    let rest = if rest.starts_with('[') {
        &rest[rest.find(']')? + 1..]
    } else {
        rest
    };
    let rest = rest.trim_start().strip_prefix('{')?;
    Some(rest[..rest.find('}')?].trim())
}

/// ソースから、使うエンジンを決める。コメントの指定が最優先。なければ、クラスやパッケージから推定し、
/// それも無ければpdfLaTeX。
pub fn detect(source: &str) -> Engine {
    if let Some(engine) = magic_comment(source) {
        return engine;
    }
    if let Some(class) = document_class(source) {
        if class.starts_with("ltj") || class.starts_with("bxjs") {
            return Engine::Lualatex;
        }
        if matches!(
            class,
            "jsarticle" | "jsbook" | "jsreport" | "jarticle" | "jbook" | "jreport"
        ) {
            return Engine::Platex;
        }
    }
    if source.contains("{luatexja") || source.contains("\\directlua") {
        Engine::Lualatex
    } else if source.contains("{xeCJK}") || source.contains("{fontspec}") {
        Engine::Xelatex
    } else {
        Engine::Pdflatex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_magic_comment_wins() {
        let src = "% !TEX program = xelatex\n\\documentclass{ltjsarticle}\n";
        assert_eq!(detect(src), Engine::Xelatex);
        assert_eq!(
            detect("%!TEX TS-program = LuaLaTeX\n\\documentclass{article}"),
            Engine::Lualatex
        );
        assert_eq!(detect("% !tex program=uplatex\n"), Engine::Uplatex);
    }

    #[test]
    fn an_unknown_magic_comment_falls_back_to_detection() {
        assert_eq!(
            detect("% !TEX program = nonsense\n\\documentclass{article}"),
            Engine::Pdflatex
        );
    }

    #[test]
    fn japanese_classes_pick_their_engines() {
        assert_eq!(
            detect("\\documentclass[a4paper]{ltjsarticle}"),
            Engine::Lualatex
        );
        assert_eq!(detect("\\documentclass{bxjsarticle}"), Engine::Lualatex);
        assert_eq!(detect("\\documentclass[11pt]{jsarticle}"), Engine::Platex);
    }

    #[test]
    fn packages_hint_the_engine() {
        assert_eq!(
            detect("\\documentclass{article}\n\\usepackage{luatexja}"),
            Engine::Lualatex
        );
        assert_eq!(
            detect("\\documentclass{article}\n\\usepackage{fontspec}"),
            Engine::Xelatex
        );
        assert_eq!(
            detect("\\documentclass{article}\n\\usepackage{xeCJK}"),
            Engine::Xelatex
        );
    }

    #[test]
    fn plain_documents_use_pdflatex() {
        assert_eq!(
            detect("\\documentclass{article}\n\\begin{document}x\\end{document}"),
            Engine::Pdflatex
        );
        assert_eq!(detect(""), Engine::Pdflatex);
    }

    #[test]
    fn the_magic_comment_must_be_near_the_top() {
        let late = format!("{}% !TEX program = xelatex\n", "\n".repeat(30));
        assert_eq!(detect(&late), Engine::Pdflatex);
    }

    #[test]
    fn latexmk_arguments_per_engine() {
        assert_eq!(Engine::Pdflatex.latexmk_args(), ["-pdf"]);
        assert_eq!(Engine::Xelatex.latexmk_args(), ["-pdfxe"]);
        assert_eq!(Engine::Lualatex.latexmk_args(), ["-pdflua"]);
        let platex = Engine::Platex.latexmk_args();
        assert_eq!(platex[0], "-pdfdvi");
        assert!(platex.iter().any(|a| a.contains("platex %O %S")));
        assert!(platex.iter().any(|a| a.contains("dvipdfmx")));
        assert!(
            Engine::Uplatex
                .latexmk_args()
                .iter()
                .any(|a| a.contains("uplatex %O %S"))
        );
    }
}
