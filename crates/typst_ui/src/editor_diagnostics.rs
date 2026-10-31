//! Shows Typst errors and warnings in the editor (underlines, hovers and Zed's diagnostics views),
//! the same way a language server's diagnostics are shown.

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use gpui::{App, Entity};
use language::{
    Diagnostic as EditorDiagnostic, DiagnosticEntry, DiagnosticMessage, DiagnosticSourceKind,
};
use lsp::{DiagnosticSeverity, LanguageServerId};
use project::Project;
use text::{PointUtf16, Unclipped};
use typst_build::{Diagnostic, Level};

/// The id Typst diagnostics are stored under. Zed groups diagnostics by language server; real
/// servers are numbered upwards from 0, so this value stays clear of them.
const TYPST_SERVER_ID: LanguageServerId = LanguageServerId(usize::MAX / 2);

/// The diagnostics of one document published to a project. Publishing replaces the previous set,
/// including clearing files that no longer have problems.
pub(crate) struct EditorDiagnostics {
    project: Entity<Project>,
    published: HashSet<PathBuf>,
}

impl EditorDiagnostics {
    pub(crate) fn new(project: Entity<Project>) -> Self {
        Self {
            project,
            published: HashSet::new(),
        }
    }

    pub(crate) fn publish<'a>(
        &mut self,
        diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
        cx: &mut App,
    ) {
        let mut by_path = to_editor_diagnostics(diagnostics);
        for stale in self.published.drain() {
            by_path.entry(stale).or_default();
        }
        self.published = by_path
            .iter()
            .filter(|(_, entries)| !entries.is_empty())
            .map(|(path, _)| path.clone())
            .collect();
        let lsp_store = self.project.read(cx).lsp_store();
        lsp_store.update(cx, |lsp_store, cx| {
            for (path, entries) in by_path {
                if let Err(error) = lsp_store.update_diagnostic_entries(
                    TYPST_SERVER_ID,
                    path,
                    None,
                    None,
                    entries,
                    cx,
                ) {
                    log::warn!("failed to show Typst diagnostics: {error:#}");
                }
            }
        });
    }
}

/// Groups the diagnostics that have a location by file, as editor diagnostics.
fn to_editor_diagnostics<'a>(
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
) -> BTreeMap<PathBuf, Vec<DiagnosticEntry<Unclipped<PointUtf16>>>> {
    let mut by_path: BTreeMap<PathBuf, Vec<_>> = BTreeMap::new();
    for (group_id, diagnostic) in diagnostics.into_iter().enumerate() {
        let (Some(path), Some(range)) = (&diagnostic.path, diagnostic.range) else {
            continue;
        };
        let point =
            |(row, column): (usize, usize)| Unclipped(PointUtf16::new(row as u32, column as u32));
        let mut message = diagnostic.message.clone();
        for hint in &diagnostic.hints {
            message.push_str("\nhint: ");
            message.push_str(hint);
        }
        by_path
            .entry(path.clone())
            .or_default()
            .push(DiagnosticEntry::new(
                point(range.start)..point(range.end),
                EditorDiagnostic {
                    source: Some("typst".to_string()),
                    severity: match diagnostic.severity {
                        Level::Error => DiagnosticSeverity::ERROR,
                        Level::Warning => DiagnosticSeverity::WARNING,
                    },
                    message: DiagnosticMessage::plain(message),
                    group_id,
                    is_primary: true,
                    source_kind: DiagnosticSourceKind::Other,
                    underline: true,
                    ..EditorDiagnostic::default()
                },
            ));
    }
    by_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst_build::Utf16Range;

    fn located(path: &str, start: (usize, usize), end: (usize, usize)) -> Diagnostic {
        Diagnostic {
            path: Some(PathBuf::from(path)),
            range: Some(Utf16Range { start, end }),
            hints: vec!["try this".to_string()],
            ..Diagnostic::error("broken".to_string())
        }
    }

    #[test]
    fn diagnostics_are_grouped_by_file_and_keep_their_hints() {
        let diagnostics = [
            located("/a/main.typ", (3, 1), (3, 8)),
            Diagnostic::error("no location".to_string()),
            located("/a/lib.typ", (0, 0), (0, 2)),
        ];
        let by_path = to_editor_diagnostics(&diagnostics);
        assert_eq!(
            by_path.len(),
            2,
            "diagnostics without a location are skipped"
        );
        let main = &by_path[&PathBuf::from("/a/main.typ")][0];
        assert_eq!(main.range.start.0, PointUtf16::new(3, 1));
        assert_eq!(main.range.end.0, PointUtf16::new(3, 8));
        assert_eq!(main.diagnostic.severity, DiagnosticSeverity::ERROR);
        assert_eq!(main.diagnostic.message.as_str(), "broken\nhint: try this");
    }
}
