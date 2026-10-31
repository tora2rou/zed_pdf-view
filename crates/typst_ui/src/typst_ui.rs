//! Typst support for Tuzuri.
//!
//! Live preview: the text in the editor (saved or not) is compiled shortly after each edit and
//! shown with Chihayaburu's `PdfView`. Ctrl/Cmd-clicking the preview jumps to the source that
//! produced the clicked text.
//!
//! Debug mode: a dock panel that lists the errors and warnings of the document being edited.

mod diagnostics_panel;
mod document;
mod preview;

use std::path::{Path, PathBuf};

use editor::Editor;
use gpui::{App, AsyncWindowContext, Entity, EntityId, WeakEntity, actions};
use language::{Buffer, Point};
use typst_build::SourcePosition;
use workspace::Workspace;

pub use diagnostics_panel::{ToggleDiagnosticsPanel, TypstDiagnosticsPanel};
pub use document::{PdfRequest, TypstDocument, TypstDocumentEvent};
pub use preview::TypstPreview;

actions!(
    typst,
    [
        /// Opens a live preview of the Typst document in the current pane.
        OpenPreview,
        /// Opens a live preview of the Typst document next to the editor.
        OpenPreviewToTheSide,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| register(workspace))
        .detach();
}

fn register(workspace: &mut Workspace) {
    workspace.register_action(|workspace, _: &OpenPreview, window, cx| {
        if let Some(main) = active_typst_file(workspace, cx) {
            let pane = workspace.active_pane().clone();
            preview::open_preview(workspace, main, pane, true, window, cx);
        }
    });
    workspace.register_action(|workspace, _: &OpenPreviewToTheSide, window, cx| {
        if let Some(main) = active_typst_file(workspace, cx) {
            let origin = workspace.active_pane().clone();
            let pane = workspace.adjacent_pane_of(&origin, window, cx);
            preview::open_preview(workspace, main, pane, false, window, cx);
        }
    });
    diagnostics_panel::register(workspace);
}

fn is_typst(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("typ"))
}

fn buffer_abs_path(buffer: &Entity<Buffer>, cx: &App) -> Option<PathBuf> {
    Some(buffer.read(cx).file()?.as_local()?.abs_path(cx))
}

/// Paths are compared after resolving symlinks, because the compiler reports resolved paths.
fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The path of the Typst file in the active editor, if any.
fn active_typst_file(workspace: &Workspace, cx: &App) -> Option<PathBuf> {
    let editor = workspace.active_item(cx)?.act_as::<Editor>(cx)?;
    let buffer = editor.read(cx).buffer().read(cx).as_singleton()?;
    buffer_abs_path(&buffer, cx).filter(|path| is_typst(path))
}

/// Opens the source file and moves the cursor to `position`. The file opens in a pane that does
/// not contain `avoid_item` (the preview the user clicked), when there is one.
async fn go_to_source(
    workspace: WeakEntity<Workspace>,
    position: SourcePosition,
    avoid_item: Option<EntityId>,
    cx: &mut AsyncWindowContext,
) {
    let open = workspace.update_in(cx, |workspace, window, cx| {
        let project_path = workspace
            .project()
            .read(cx)
            .find_project_path(&position.path, cx)?;
        let pane = workspace
            .panes()
            .iter()
            .find(|pane| {
                pane.read(cx)
                    .items()
                    .all(|item| Some(item.item_id()) != avoid_item)
            })
            .map(|pane| pane.downgrade());
        Some(workspace.open_path(project_path, pane, true, window, cx))
    });
    let Ok(Some(open)) = open else {
        return;
    };
    let Ok(item) = open.await else {
        return;
    };
    if let Some(editor) = item.downcast::<Editor>() {
        editor
            .update_in(cx, |editor, window, cx| {
                let Some(buffer) = editor.buffer().read(cx).as_singleton() else {
                    return;
                };
                let point = buffer_point(&buffer.read(cx).text_snapshot(), &position);
                editor.go_to_singleton_buffer_point(point, window, cx);
            })
            .ok();
    }
}

/// The buffer position of a compiler position. Typst counts columns in characters; Zed counts bytes.
fn buffer_point(snapshot: &text::BufferSnapshot, position: &SourcePosition) -> Point {
    let max = snapshot.max_point();
    let row = (position.line.saturating_sub(1) as u32).min(max.row);
    let line_end = Point::new(row, snapshot.line_len(row));
    let line: String = snapshot
        .text_for_range(Point::new(row, 0)..line_end)
        .collect();
    let column = byte_column(&line, position.column.saturating_sub(1));
    Point::new(row, column as u32)
}

/// The byte offset of the `chars`-th character of `line`, or the line's length if it is shorter.
fn byte_column(line: &str, chars: usize) -> usize {
    line.char_indices()
        .nth(chars)
        .map_or(line.len(), |(offset, _)| offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DEBOUNCE;
    use fs::FakeFs;
    use gpui::AppContext as _;
    use gpui::{TestAppContext, VisualTestContext};
    use pdf_viewer::PdfViewEvent;
    use project::Project;
    use std::sync::Arc;
    use workspace::{AppState, MultiWorkspace};

    const PAGE: &str =
        "#set page(width: 200pt, height: 200pt, margin: 10pt)\n#set text(size: 20pt)\n";

    fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
        cx.update(|cx| {
            cx.set_global(db::AppDatabase::test_new());
            let state = AppState::test(cx);
            editor::init(cx);
            crate::init(cx);
            state
        })
    }

    /// Writes `main.typ` both to a real temporary directory (the compiler reads the disk) and to
    /// the fake file system the project uses, then opens it in an editor.
    async fn open_main<'a>(
        name: &str,
        text: &str,
        cx: &'a mut TestAppContext,
    ) -> (
        PathBuf,
        Entity<Workspace>,
        Entity<Editor>,
        &'a mut VisualTestContext,
    ) {
        open_project(name, &[("main.typ", text)], cx).await
    }

    /// Writes `files` both to a real temporary directory (the compiler reads the disk) and to the
    /// fake file system the project uses, then opens `main.typ` in an editor.
    async fn open_project<'a>(
        name: &str,
        files: &[(&str, &str)],
        cx: &'a mut TestAppContext,
    ) -> (
        PathBuf,
        Entity<Workspace>,
        Entity<Editor>,
        &'a mut VisualTestContext,
    ) {
        init_test(cx);
        let dir = std::env::temp_dir().join(format!("typst_ui_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let mut tree = serde_json::Map::new();
        for (file, text) in files {
            std::fs::write(dir.join(file), text).unwrap();
            tree.insert(file.to_string(), serde_json::Value::from(*text));
        }
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(&dir, serde_json::Value::Object(tree)).await;
        let project = Project::test(fs, [dir.as_path()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
        let workspace =
            multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());
        let main = dir.join("main.typ");
        let open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(main.clone(), Default::default(), window, cx)
        });
        let editor = open.await.unwrap().downcast::<Editor>().unwrap();
        (main, workspace, editor, cx)
    }

    fn open_preview_beside(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Entity<TypstPreview> {
        workspace.update_in(cx, |workspace, window, cx| {
            let main = active_typst_file(workspace, cx).expect("a Typst editor is active");
            let origin = workspace.active_pane().clone();
            let pane = workspace.adjacent_pane_of(&origin, window, cx);
            preview::open_preview(workspace, main, pane.clone(), false, window, cx);
            pane.read(cx)
                .items_of_type::<TypstPreview>()
                .next()
                .unwrap()
        })
    }

    fn settle(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.executor().advance_clock(DEBOUNCE * 2);
        cx.run_until_parked();
    }

    fn page_count(preview: &Entity<TypstPreview>, cx: &mut VisualTestContext) -> Option<usize> {
        preview.read_with(cx, |preview, cx| {
            Some(preview.viewer.as_ref()?.read(cx).document().page_count())
        })
    }

    fn append(editor: &Entity<Editor>, text: &str, cx: &mut VisualTestContext) {
        editor.update_in(cx, |editor, window, cx| {
            editor.move_to_end(&Default::default(), window, cx);
            editor.insert(text, window, cx);
        });
    }

    /// Without a preview, only errors are checked and no PDF is kept. A preview asks for the PDF.
    #[gpui::test]
    async fn the_pdf_is_only_made_while_a_preview_wants_it(cx: &mut TestAppContext) {
        let (main, workspace, _, cx) =
            open_main("check_only", &format!("{PAGE}#unknown()\n"), cx).await;
        let project = workspace.read_with(cx, |workspace, _| workspace.project().clone());
        let document = cx.update(|_, cx| TypstDocument::for_main(&main, &project, cx));
        document.update(cx, |document, cx| document.ensure_compiled(cx));
        settle(cx);
        document.read_with(cx, |document, _| {
            assert_eq!(document.errors().len(), 1);
            assert!(document.pdf().is_none());
        });

        let preview = open_preview_beside(&workspace, cx);
        settle(cx);
        assert_eq!(
            preview.read_with(cx, |preview, _| preview.document.clone()),
            document,
            "the preview shares the document"
        );
    }

    /// An imported file belongs to the document that imports it.
    #[gpui::test]
    async fn an_imported_file_belongs_to_the_importing_document(cx: &mut TestAppContext) {
        let (main, workspace, _, cx) = open_project(
            "for_file",
            &[
                ("main.typ", "#import \"lib.typ\": f\n#f()\n"),
                ("lib.typ", "#let f() = [x]\n"),
            ],
            cx,
        )
        .await;
        let project = workspace.read_with(cx, |workspace, _| workspace.project().clone());
        let document = cx.update(|_, cx| TypstDocument::for_main(&main, &project, cx));
        document.update(cx, |document, cx| document.ensure_compiled(cx));
        settle(cx);
        let lib = main.with_file_name("lib.typ");
        let found = cx.update(|_, cx| TypstDocument::for_file(&lib, &project, cx));
        assert_eq!(found, document);
        let other = main.with_file_name("other.typ");
        let unrelated = cx.update(|_, cx| TypstDocument::for_file(&other, &project, cx));
        assert_ne!(unrelated, document);
    }

    fn open_diagnostics_panel(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Entity<TypstDiagnosticsPanel> {
        workspace.update_in(cx, |workspace, window, cx| {
            let workspace_entity = cx.entity();
            let panel =
                cx.new(|cx| TypstDiagnosticsPanel::new(&workspace_entity, workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            workspace.toggle_panel_focus::<TypstDiagnosticsPanel>(window, cx);
            panel
        })
    }

    /// Debug mode lists the problems of the document being edited, including files it imports,
    /// jumps to a problem when it is clicked, and lets go of the document when hidden.
    #[gpui::test]
    async fn the_diagnostics_panel_follows_the_active_document(cx: &mut TestAppContext) {
        let (main, workspace, editor, cx) = open_project(
            "panel",
            &[
                ("main.typ", "#import \"lib.typ\": f\n#f()\n\n#unknown()\n"),
                ("lib.typ", "#let f() = [x]\n"),
            ],
            cx,
        )
        .await;
        let panel = open_diagnostics_panel(&workspace, cx);
        settle(cx);
        let document = panel.read_with(cx, |panel, _| panel.document.clone().expect("a document"));
        let error = document.read_with(cx, |document, _| {
            assert_eq!(document.main(), main.as_path());
            assert_eq!(document.errors().len(), 1, "{:?}", document.errors());
            document.errors()[0].clone()
        });
        assert_eq!(error.line, Some(4));

        // Editing an imported file keeps showing the document that imports it.
        let open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                main.with_file_name("lib.typ"),
                Default::default(),
                window,
                cx,
            )
        });
        open.await.unwrap();
        settle(cx);
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.document.clone()),
            Some(document.clone())
        );

        // Clicking a problem moves the cursor to it.
        panel.update_in(cx, |panel, window, cx| {
            panel.jump_to(main.clone(), 4, 2, window, cx);
        });
        settle(cx);
        editor.update_in(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            let head = editor.selections.newest::<Point>(&snapshot).head();
            assert_eq!(head, Point::new(3, 1));
        });

        // Hiding the panel turns debug mode off and releases the compiler.
        let weak = document.downgrade();
        drop(document);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_panel_focus::<TypstDiagnosticsPanel>(window, cx);
            workspace.close_panel::<TypstDiagnosticsPanel>(window, cx);
        });
        settle(cx);
        assert!(panel.read_with(cx, |panel, _| panel.document.is_none()));
        assert!(weak.upgrade().is_none(), "the document is released");
    }

    #[gpui::test]
    async fn the_preview_follows_unsaved_edits(cx: &mut TestAppContext) {
        let (main, workspace, editor, cx) = open_main("edits", &format!("{PAGE}= One\n"), cx).await;
        let preview = open_preview_beside(&workspace, cx);
        settle(cx);
        assert_eq!(page_count(&preview, cx), Some(1));

        append(&editor, "#pagebreak()\n= Two\n", cx);
        settle(cx);
        assert_eq!(
            page_count(&preview, cx),
            Some(2),
            "unsaved text is compiled"
        );
        assert!(std::fs::read_to_string(&main).unwrap().ends_with("= One\n"));

        // While the document is broken, the last good output stays visible.
        append(&editor, "#unknown()\n", cx);
        settle(cx);
        preview.read_with(cx, |preview, cx| {
            let errors = preview.document.read(cx).errors();
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert_eq!(errors[0].line, Some(6));
        });
        assert_eq!(page_count(&preview, cx), Some(2));
    }

    #[gpui::test]
    async fn opening_the_preview_twice_reuses_it(cx: &mut TestAppContext) {
        let (_, workspace, editor, cx) = open_main("twice", &format!("{PAGE}= One\n"), cx).await;
        let first = open_preview_beside(&workspace, cx);
        settle(cx);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.activate_item(&editor, true, true, window, cx);
        });
        let second = open_preview_beside(&workspace, cx);
        assert_eq!(first.entity_id(), second.entity_id());
    }

    #[gpui::test]
    async fn ctrl_clicking_the_preview_moves_the_cursor_to_the_source(cx: &mut TestAppContext) {
        let (_, workspace, editor, cx) =
            open_main("jump", &format!("{PAGE}First\n\nSecond\n"), cx).await;
        let preview = open_preview_beside(&workspace, cx);
        settle(cx);
        let viewer = preview.read_with(cx, |preview, _| preview.viewer.clone().unwrap());

        viewer.update(cx, |_, cx| {
            cx.emit(PdfViewEvent::ModifiedClick {
                page: 0,
                x: 15.0,
                y: 55.0,
            })
        });
        settle(cx);

        editor.update_in(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            let head = editor.selections.newest::<Point>(&snapshot).head();
            assert_eq!(head.row, 4, "the line with \"Second\"");
        });
    }

    #[test]
    fn detects_typst_files() {
        assert!(is_typst(Path::new("a/main.typ")));
        assert!(is_typst(Path::new("MAIN.TYP")));
        assert!(!is_typst(Path::new("main.md")));
    }

    #[test]
    fn byte_column_counts_characters() {
        assert_eq!(byte_column("abc", 0), 0);
        assert_eq!(byte_column("abc", 2), 2);
        // Japanese characters are 3 bytes each in UTF-8.
        assert_eq!(byte_column("日本語です", 2), 6);
        assert_eq!(byte_column("日本", 10), 6, "clamped to the line end");
    }
}
