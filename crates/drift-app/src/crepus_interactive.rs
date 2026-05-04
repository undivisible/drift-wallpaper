//! Bridges `.crepus` `@mouseup` attributes to GPUI entities.
//!
//! We still use `crepuscularity-runtime` from crates.io. This module only restores the
//! app-specific interactive dispatch glue that is no longer exposed by runtime 0.3.

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
