//! A tab that shows the PDF of a Typst document and follows its edits.

use std::{path::PathBuf, sync::Arc};

use file_icons::FileIcons;
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Task, WeakEntity, Window, div,
};
use pdf_core::PdfDocument;
use pdf_viewer::{PdfView, PdfViewEvent};
use ui::prelude::*;
use workspace::{Pane, Workspace, item::Item};

use crate::{
    document::{PdfRequest, TypstDocument, TypstDocumentEvent},
    go_to_source,
};

pub(crate) fn open_preview(
    workspace: &mut Workspace,
    main: PathBuf,
    pane: Entity<Pane>,
    focus: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let document = TypstDocument::for_main(&main, workspace.project(), cx);
    let existing = pane
        .read(cx)
        .items_of_type::<TypstPreview>()
        .find(|preview| preview.read(cx).document == document);
    if let Some(existing) = existing {
        pane.update(cx, |pane, cx| {
            if let Some(index) = pane.index_for_item(&existing) {
                pane.activate_item(index, true, focus, window, cx);
            }
        });
        return;
    }
    let workspace_handle = workspace.weak_handle();
    let preview = cx.new(|cx| TypstPreview::new(document, workspace_handle, window, cx));
    pane.update(cx, |pane, cx| {
        pane.add_item(Box::new(preview), focus, focus, None, window, cx)
    });
}

pub struct TypstPreview {
    pub(crate) document: Entity<TypstDocument>,
    _pdf_request: PdfRequest,
    workspace: WeakEntity<Workspace>,
    pub(crate) viewer: Option<Entity<PdfView>>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
    jump_task: Task<()>,
}

impl TypstPreview {
    fn new(
        document: Entity<TypstDocument>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let pdf_request = document.update(cx, |document, cx| document.request_pdf(cx));
        let subscription = cx.subscribe_in(
            &document,
            window,
            |this, _, _: &TypstDocumentEvent, window, cx| {
                this.sync_viewer(window, cx);
                cx.notify();
            },
        );
        let mut this = Self {
            document,
            _pdf_request: pdf_request,
            workspace,
            viewer: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![subscription],
            jump_task: Task::ready(()),
        };
        this.sync_viewer(window, cx);
        this
    }

    /// Shows the document's latest PDF if it is not shown yet.
    fn sync_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pdf) = self.document.read(cx).pdf().cloned() else {
            return;
        };
        if let Some(viewer) = &self.viewer {
            if !Arc::ptr_eq(viewer.read(cx).document(), &pdf) {
                viewer.update(cx, |viewer, cx| viewer.update_document(pdf, window, cx));
            }
            return;
        }
        self.show_first(pdf, window, cx);
    }

    fn show_first(&mut self, pdf: Arc<PdfDocument>, window: &mut Window, cx: &mut Context<Self>) {
        let viewer = cx.new(|cx| PdfView::new(pdf, cx));
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
        let lookup = self.document.read(cx).source_at(page, x, y, cx);
        let workspace = self.workspace.clone();
        let preview_id = cx.entity_id();
        self.jump_task = cx.spawn_in(window, async move |_, cx| {
            if let Some(position) = lookup.await {
                go_to_source(workspace, position, Some(preview_id), cx).await;
            }
        });
    }

    fn file_name(&self, cx: &App) -> String {
        self.document
            .read(cx)
            .main()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn render_status(&self, cx: &App) -> Option<impl IntoElement> {
        let document = self.document.read(cx);
        let errors = document.errors();
        let warnings = document.warnings();
        let (text, color) = if let Some(first) = errors.first() {
            let more = match errors.len() {
                1 => String::new(),
                count => format!(" (+{} more)", count - 1),
            };
            (format!("{}{more}", first.one_line()), Color::Error)
        } else if document.is_compiling() && self.viewer.is_none() {
            ("Compiling…".to_string(), Color::Muted)
        } else if !warnings.is_empty() {
            let count = warnings.len();
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

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        format!("Preview {}", self.file_name(cx)).into()
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        Some(
            self.document
                .read(cx)
                .main()
                .to_string_lossy()
                .into_owned()
                .into(),
        )
    }

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        FileIcons::get_icon(self.document.read(cx).main(), cx).map(Icon::from_path)
    }
}
