//! Live preview of Typst documents. The text in the editor (saved or not) is compiled shortly after
//! each edit and shown with Chihayaburu's `PdfView`. Ctrl/Cmd-clicking the preview jumps to the
//! source that produced the clicked text.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use editor::Editor;
use file_icons::FileIcons;
use gpui::{
    App, AppContext as _, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, WeakEntity,
    Window, actions, div,
};
use language::{Buffer, BufferEvent, Point};
use parking_lot::Mutex;
use pdf_core::PdfDocument;
use pdf_viewer::{PdfView, PdfViewEvent};
use project::{Project, buffer_store::BufferStoreEvent};
use typst_build::{Diagnostic, Level, Session, SourcePosition};
use ui::prelude::*;
use workspace::{Pane, Workspace, item::Item};

actions!(
    typst,
    [
        /// Opens a live preview of the Typst document in the current pane.
        OpenPreview,
        /// Opens a live preview of the Typst document next to the editor.
        OpenPreviewToTheSide,
    ]
);

/// How long the text must stay unchanged before it is compiled.
const DEBOUNCE: Duration = Duration::from_millis(300);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| register(workspace))
        .detach();
}

fn register(workspace: &mut Workspace) {
    workspace.register_action(|workspace, _: &OpenPreview, window, cx| {
        if let Some(main) = active_typst_file(workspace, cx) {
            let pane = workspace.active_pane().clone();
            open_preview(workspace, main, pane, true, window, cx);
        }
    });
    workspace.register_action(|workspace, _: &OpenPreviewToTheSide, window, cx| {
        if let Some(main) = active_typst_file(workspace, cx) {
            let origin = workspace.active_pane().clone();
            let pane = workspace.adjacent_pane_of(&origin, window, cx);
            open_preview(workspace, main, pane, false, window, cx);
        }
    });
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

fn active_typst_file(workspace: &Workspace, cx: &App) -> Option<PathBuf> {
    let editor = workspace.active_item(cx)?.act_as::<Editor>(cx)?;
    let buffer = editor.read(cx).buffer().read(cx).as_singleton()?;
    buffer_abs_path(&buffer, cx).filter(|path| is_typst(path))
}

fn open_preview(
    workspace: &mut Workspace,
    main: PathBuf,
    pane: Entity<Pane>,
    focus: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let existing = pane
        .read(cx)
        .items_of_type::<TypstPreview>()
        .find(|preview| preview.read(cx).main == main);
    if let Some(existing) = existing {
        pane.update(cx, |pane, cx| {
            if let Some(index) = pane.index_for_item(&existing) {
                pane.activate_item(index, true, focus, window, cx);
            }
        });
        return;
    }
    let project = workspace.project().clone();
    let workspace_handle = workspace.weak_handle();
    let preview = cx.new(|cx| TypstPreview::new(main, project, workspace_handle, window, cx));
    pane.update(cx, |pane, cx| {
        pane.add_item(Box::new(preview), focus, focus, None, window, cx)
    });
}

/// A tab that shows the compiled output of one Typst document and keeps it up to date.
pub struct TypstPreview {
    main: PathBuf,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    session: Option<Arc<Mutex<Session>>>,
    viewer: Option<Entity<PdfView>>,
    focus_handle: FocusHandle,
    /// Files read by the last compilation (resolved paths). Edits to these trigger a recompile.
    dependencies: HashSet<PathBuf>,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    compiling: bool,
    /// An edit arrived while compiling, so compile again once the current run finishes.
    recompile: bool,
    buffer_subscriptions: HashMap<EntityId, Subscription>,
    _subscriptions: Vec<Subscription>,
    debounce: Task<()>,
    compile_task: Task<()>,
    jump_task: Task<()>,
}

impl TypstPreview {
    fn new(
        main: PathBuf,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (session, errors) = match Session::new(&main) {
            Ok(session) => (Some(Arc::new(Mutex::new(session))), Vec::new()),
            Err(failure) => (None, failure.errors),
        };
        let buffer_store = project.read(cx).buffer_store().clone();
        let buffers_added = cx.subscribe_in(
            &buffer_store,
            window,
            |this, _, event: &BufferStoreEvent, window, cx| {
                if let BufferStoreEvent::BufferAdded(_) = event {
                    this.watch_buffers(window, cx);
                }
            },
        );
        let mut this = Self {
            dependencies: HashSet::from([normalize(&main)]),
            main,
            project,
            workspace,
            session,
            viewer: None,
            focus_handle: cx.focus_handle(),
            errors,
            warnings: Vec::new(),
            compiling: false,
            recompile: false,
            buffer_subscriptions: HashMap::new(),
            _subscriptions: vec![buffers_added],
            debounce: Task::ready(()),
            compile_task: Task::ready(()),
            jump_task: Task::ready(()),
        };
        this.watch_buffers(window, cx);
        this.compile(window, cx);
        this
    }

    /// Open buffers of the files this document depends on, with their resolved paths.
    fn dependency_buffers(&self, cx: &App) -> Vec<(Entity<Buffer>, PathBuf)> {
        self.project
            .read(cx)
            .buffer_store()
            .read(cx)
            .buffers()
            .filter_map(|buffer| {
                let path = normalize(&buffer_abs_path(&buffer, cx)?);
                self.dependencies.contains(&path).then_some((buffer, path))
            })
            .collect()
    }

    /// Subscribes to edits of the open buffers this document depends on.
    fn watch_buffers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let buffers = self.dependency_buffers(cx);
        let watched: HashSet<EntityId> = buffers
            .iter()
            .map(|(buffer, _)| buffer.entity_id())
            .collect();
        self.buffer_subscriptions
            .retain(|id, _| watched.contains(id));
        for (buffer, _) in buffers {
            self.buffer_subscriptions
                .entry(buffer.entity_id())
                .or_insert_with(|| {
                    cx.subscribe_in(
                        &buffer,
                        window,
                        |this, _, event: &BufferEvent, window, cx| {
                            if matches!(event, BufferEvent::Edited { .. } | BufferEvent::Reloaded) {
                                this.schedule_compile(window, cx);
                            }
                        },
                    )
                });
        }
    }

    fn schedule_compile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.debounce = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            this.update_in(cx, |this, window, cx| this.compile(window, cx))
                .ok();
        });
    }

    fn compile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if self.compiling {
            self.recompile = true;
            return;
        }
        // Only unsaved text needs to be passed; everything else is read from disk.
        let overlays: HashMap<PathBuf, String> = self
            .dependency_buffers(cx)
            .into_iter()
            .filter(|(buffer, _)| buffer.read(cx).is_dirty())
            .map(|(buffer, path)| (path, buffer.read(cx).text()))
            .collect();
        self.compiling = true;
        cx.notify();

        let compile = cx.background_spawn(async move {
            let result = session.lock().compile(overlays);
            result.map(|compiled| {
                let document = PdfDocument::from_bytes(compiled.pdf).map(Arc::new);
                (document, compiled.warnings, compiled.dependencies)
            })
        });
        self.compile_task = cx.spawn_in(window, async move |this, cx| {
            let result = compile.await;
            this.update_in(cx, |this, window, cx| {
                this.compiling = false;
                let dependencies = match result {
                    Ok((Ok(document), warnings, dependencies)) => {
                        this.show(document, window, cx);
                        this.errors.clear();
                        this.warnings = warnings;
                        dependencies
                    }
                    Ok((Err(error), warnings, dependencies)) => {
                        this.errors = vec![message(format!(
                            "The compiled PDF could not be opened: {error}"
                        ))];
                        this.warnings = warnings;
                        dependencies
                    }
                    Err(failure) => {
                        this.errors = failure.errors;
                        this.warnings = failure.warnings;
                        failure.dependencies
                    }
                };
                this.dependencies = dependencies.iter().map(|path| normalize(path)).collect();
                this.dependencies.insert(normalize(&this.main));
                this.watch_buffers(window, cx);
                if std::mem::take(&mut this.recompile) {
                    this.compile(window, cx);
                }
                cx.notify();
            })
            .ok();
        });
    }

    fn show(&mut self, document: Arc<PdfDocument>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.update_document(document, window, cx)
            });
            return;
        }
        let viewer = cx.new(|cx| PdfView::new(document, cx));
        let subscription = cx.subscribe_in(&viewer, window, Self::on_viewer_event);
        self._subscriptions.push(subscription);
        let had_focus = self.focus_handle.is_focused(window);
        self.viewer = Some(viewer.clone());
        if had_focus {
            window.focus(&viewer.focus_handle(cx), cx);
        }
    }

    fn on_viewer_event(
        &mut self,
        _viewer: &Entity<PdfView>,
        event: &PdfViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PdfViewEvent::ModifiedClick { page, x, y } = *event;
        let Some(session) = self.session.clone() else {
            return;
        };
        // The session is locked while compiling, so look the position up off the UI thread.
        let lookup = cx.background_spawn(async move { session.lock().source_at(page, x, y) });
        let workspace = self.workspace.clone();
        let preview_id = cx.entity_id();
        self.jump_task = cx.spawn_in(window, async move |_, cx| {
            let Some(position) = lookup.await else {
                return;
            };
            let open = workspace.update_in(cx, |workspace, window, cx| {
                open_source(workspace, &position, preview_id, window, cx)
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
        });
    }
}

/// Opens the source file, preferring a pane other than the one showing the preview.
fn open_source(
    workspace: &mut Workspace,
    position: &SourcePosition,
    preview_id: EntityId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Option<Task<anyhow::Result<Box<dyn workspace::ItemHandle>>>> {
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
                .all(|item| item.item_id() != preview_id)
        })
        .map(|pane| pane.downgrade());
    Some(workspace.open_path(project_path, pane, true, window, cx))
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

fn message(text: String) -> Diagnostic {
    Diagnostic {
        severity: Level::Error,
        message: text,
        file: None,
        path: None,
        line: None,
        column: None,
        hints: Vec::new(),
    }
}

impl TypstPreview {
    fn file_name(&self) -> String {
        self.main
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn render_status(&self, cx: &App) -> Option<impl IntoElement> {
        let (text, color) = if let Some(first) = self.errors.first() {
            let more = match self.errors.len() {
                1 => String::new(),
                count => format!(" (+{} more)", count - 1),
            };
            (format!("{}{more}", first.one_line()), Color::Error)
        } else if self.compiling && self.viewer.is_none() {
            ("Compiling…".to_string(), Color::Muted)
        } else if !self.warnings.is_empty() {
            let count = self.warnings.len();
            let noun = if count == 1 { "warning" } else { "warnings" };
            (format!("{count} {noun}"), Color::Warning)
        } else {
            return None;
        };
        Some(
            div()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .child(Label::new(text).color(color).single_line()),
        )
    }
}

impl Render for TypstPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("TypstPreview")
            .key_context("TypstPreview")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .children(self.render_status(cx))
            .child(match &self.viewer {
                Some(viewer) => div().flex_1().min_h_0().child(viewer.clone()),
                None => div().flex_1(),
            })
    }
}

impl Focusable for TypstPreview {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.viewer {
            Some(viewer) => viewer.focus_handle(cx),
            None => self.focus_handle.clone(),
        }
    }
}

impl EventEmitter<()> for TypstPreview {}

impl Item for TypstPreview {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        format!("Preview {}", self.file_name()).into()
    }

    fn tab_tooltip_text(&self, _cx: &App) -> Option<SharedString> {
        Some(self.main.to_string_lossy().into_owned().into())
    }

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        FileIcons::get_icon(&self.main, cx).map(Icon::from_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs::FakeFs;
    use gpui::{TestAppContext, VisualTestContext};
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
        init_test(cx);
        let dir = std::env::temp_dir().join(format!("typst_preview_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let main = dir.join("main.typ");
        std::fs::write(&main, text).unwrap();
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(&dir, serde_json::json!({ "main.typ": text }))
            .await;
        let project = Project::test(fs, [dir.as_path()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
        let workspace =
            multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());
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
            open_preview(workspace, main, pane.clone(), false, window, cx);
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
        preview.read_with(cx, |preview, _| {
            assert_eq!(preview.errors.len(), 1, "{:?}", preview.errors);
            assert_eq!(preview.errors[0].line, Some(6));
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
