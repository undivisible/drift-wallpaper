//! Bridges `.crepus` `@mouseup` attributes to GPUI entities (vendored; upstream
//! `crepuscularity-runtime` 0.3 on crates.io dropped the interactive renderer).

use gpui::{Context, MouseUpEvent, Window};

pub trait CrepusMouseDispatch: Sized {
    fn dispatch_crepus_mouse_up(
        &mut self,
        action: &str,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

pub(crate) fn normalize_action_handler(raw: &str) -> String {
    let s = raw.trim();
    s.strip_prefix("Self::").unwrap_or(s).to_string()
}
