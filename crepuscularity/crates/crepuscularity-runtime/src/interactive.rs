//! Interactive templates: dispatch string action names from `@mouseup` / `@click` to GPUI entity handlers.

use gpui::{ClickEvent, Context, MouseUpEvent, Window};

/// Map template `@mouseup=action_name` / `@click=action_name` to Rust on an entity.
///
/// Handler names may be written as `close_window` or `Self::close_window` (both are normalized).
pub trait CrepusMouseDispatch: Sized {
    fn dispatch_crepus_mouse_up(
        &mut self,
        action: &str,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    );

    /// Optional; default is empty. Override when using `@click=…` in templates.
    fn dispatch_crepus_click(
        &mut self,
        _action: &str,
        _event: &ClickEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

/// Placeholder type for non-interactive [`super::render_nodes`] (no handlers attached).
pub struct InertCrepusDispatch;

impl CrepusMouseDispatch for InertCrepusDispatch {
    fn dispatch_crepus_mouse_up(
        &mut self,
        _: &str,
        _: &MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
    }
}

pub(crate) fn normalize_action_handler(raw: &str) -> String {
    let s = raw.trim();
    s.strip_prefix("Self::").unwrap_or(s).to_string()
}
