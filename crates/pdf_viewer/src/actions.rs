use gpui::{App, KeyBinding, actions};

actions!(
    pdf_viewer,
    [
        ZoomIn,
        ZoomOut,
        ResetZoom,
        NextPage,
        PreviousPage,
        FirstPage,
        LastPage,
        RotateClockwise,
        RotateCounterclockwise,
        ToggleDarkMode,
        ToggleFullscreen,
        ToggleSidebar,
    ]
);

/// `PdfView`のキーコンテキスト名。
pub const KEY_CONTEXT: &str = "PdfViewer";

/// 既定のキーバインドを登録する。Zed本体に組み込むときはkeymapのjsonで定義するので不要。
pub fn init(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("ctrl-=", ZoomIn, ctx),
        KeyBinding::new("ctrl-+", ZoomIn, ctx),
        KeyBinding::new("cmd-=", ZoomIn, ctx),
        KeyBinding::new("ctrl--", ZoomOut, ctx),
        KeyBinding::new("cmd--", ZoomOut, ctx),
        KeyBinding::new("ctrl-0", ResetZoom, ctx),
        KeyBinding::new("cmd-0", ResetZoom, ctx),
        KeyBinding::new("pagedown", NextPage, ctx),
        KeyBinding::new("pageup", PreviousPage, ctx),
        KeyBinding::new("right", NextPage, ctx),
        KeyBinding::new("left", PreviousPage, ctx),
        KeyBinding::new("home", FirstPage, ctx),
        KeyBinding::new("end", LastPage, ctx),
        KeyBinding::new("r", RotateClockwise, ctx),
        KeyBinding::new("shift-r", RotateCounterclockwise, ctx),
        KeyBinding::new("d", ToggleDarkMode, ctx),
        KeyBinding::new("f11", ToggleFullscreen, ctx),
        KeyBinding::new("f9", ToggleSidebar, ctx),
    ]);
}
