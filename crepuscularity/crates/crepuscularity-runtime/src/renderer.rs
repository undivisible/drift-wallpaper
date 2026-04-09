/// Runtime GPUI renderer — walks the AST and builds GPUI elements dynamically.
///
/// Supports:
/// - Dynamic theme colors via context expressions in class values
/// - GPUI animations via `animate:property={duration easing}` attributes
/// - All standard Tailwind-like classes mapped to GPUI methods
/// - Optional `@mouseup` / `@click` handlers when using [`render_nodes_interactive`]
/// - `overflow-*-scroll` via `Stateful<Div>` (`.id` + scroll), since plain `Div` cannot scroll
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gpui::{
    bounce, div, ease_in_out, ease_out_quint, linear, quadratic, rgb, Animation, AnimationExt,
    AnyElement, Context, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, WeakEntity,
};

static SCROLL_OVERFLOW_ID: AtomicU64 = AtomicU64::new(0);

use crate::ast::*;
use crate::context::{value_to_str, TemplateContext, TemplateValue};
use crate::interactive::{normalize_action_handler, CrepusMouseDispatch, InertCrepusDispatch};
use crate::styler::{apply_class_with_ctx, parse_duration_ms};

/// Load template source from [`TemplateContext::virtual_files`] or disk.
fn read_crepus_source(ctx: &TemplateContext, path: &PathBuf) -> Result<String, String> {
    if let Some(base) = ctx.base_dir.as_deref() {
        if let Ok(rel) = path.strip_prefix(base) {
            let key = rel.to_string_lossy().replace('\\', "/");
            if let Some(s) = ctx.virtual_files.get(&key) {
                return Ok(s.clone());
            }
        }
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if let Some(s) = ctx.virtual_files.get(name) {
            return Ok(s.clone());
        }
    }
    let lossy = path.to_string_lossy().to_string();
    if let Some(s) = ctx.virtual_files.get(&lossy) {
        return Ok(s.clone());
    }

    std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))
}

/// Render a list of nodes into a single `AnyElement`, threading `LetDecl`s into
/// a running context clone so later siblings see the declared variables.
pub fn render_nodes(nodes: &[Node], ctx: &TemplateContext) -> AnyElement {
    render_nodes_maybe_dispatch::<InertCrepusDispatch>(nodes, ctx, None)
}

/// Render with `@mouseup` / `@click` dispatch to [`CrepusMouseDispatch`] on `T`.
pub fn render_nodes_interactive<T: CrepusMouseDispatch + 'static>(
    nodes: &[Node],
    ctx: &TemplateContext,
    cx: &mut Context<T>,
) -> AnyElement {
    let weak = cx.weak_entity();
    render_nodes_maybe_dispatch(nodes, ctx, Some(&weak))
}

fn render_nodes_maybe_dispatch<T: CrepusMouseDispatch + 'static>(
    nodes: &[Node],
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    let mut rendered: Vec<AnyElement> = Vec::new();
    let mut run_ctx = ctx.clone();

    for node in nodes {
        if let Node::LetDecl(decl) = node {
            if decl.is_default && run_ctx.vars.contains_key(&decl.name) {
                // Default: skip if already set by parent props
            } else {
                let val = crate::eval::eval_expr(&decl.expr, &run_ctx);
                run_ctx.vars.insert(decl.name.clone(), val);
            }
        } else {
            rendered.push(render_node_impl(node, &run_ctx, ix));
        }
    }

    match rendered.len() {
        0 => div().into_any_element(),
        1 => rendered.remove(0),
        _ => {
            let mut d = div();
            for child in rendered {
                d = d.child(child);
            }
            d.into_any_element()
        }
    }
}

pub fn render_node(node: &Node, ctx: &TemplateContext) -> AnyElement {
    render_node_impl::<InertCrepusDispatch>(node, ctx, None)
}

fn render_node_impl<T: CrepusMouseDispatch + 'static>(
    node: &Node,
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    match node {
        Node::Element(el) => render_element(el, ctx, ix),
        Node::Text(parts) => {
            let text = render_text(parts, ctx);
            div().child(SharedString::from(text)).into_any_element()
        }
        Node::If(block) => render_if(block, ctx, ix),
        Node::For(block) => render_for(block, ctx, ix),
        Node::Match(block) => render_match(block, ctx, ix),
        Node::LetDecl(_) => div().into_any_element(), // handled in render_nodes_maybe_dispatch
        Node::RawText(expr) => {
            let val = crate::eval::eval_expr(expr, ctx);
            div()
                .child(SharedString::from(value_to_str(&val)))
                .into_any_element()
        }
        Node::Include(inc) => render_include(inc, ctx, ix),
    }
}

fn attach_interactive_handlers<T: CrepusMouseDispatch + 'static>(
    mut d: gpui::Div,
    handlers: &[EventHandler],
    ix: Option<&WeakEntity<T>>,
) -> gpui::Div {
    let Some(weak) = ix.map(|w| w.clone()) else {
        return d;
    };
    for h in handlers {
        match h.event.as_str() {
            "mouseup" => {
                let action = normalize_action_handler(&h.handler);
                let wk = weak.clone();
                d = d.on_mouse_up(MouseButton::Left, move |ev, window, app| {
                    let _ = wk.update(app, |this, cx| {
                        CrepusMouseDispatch::dispatch_crepus_mouse_up(
                            this, &action, ev, window, cx,
                        );
                    });
                });
            }
            // `@click` uses `StatefulInteractiveElement`; plain `Div` only registers `mouseup` here.
            "click" => {}
            _ => {}
        }
    }
    d
}

fn is_overflow_scroll_class(c: &str) -> bool {
    matches!(
        c,
        "overflow-scroll"
            | "overflow-auto"
            | "overflow-y-scroll"
            | "overflow-y-auto"
            | "overflow-x-scroll"
            | "overflow-x-auto"
    )
}

fn scroll_axes(el: &Element, ctx: &TemplateContext) -> Option<(bool, bool)> {
    let mut ox = false;
    let mut oy = false;
    let mut note = |c: &str| match c {
        "overflow-scroll" | "overflow-auto" => {
            ox = true;
            oy = true;
        }
        "overflow-y-scroll" | "overflow-y-auto" => oy = true,
        "overflow-x-scroll" | "overflow-x-auto" => ox = true,
        _ => {}
    };
    for c in &el.classes {
        note(c.as_str());
    }
    for cc in &el.conditional_classes {
        if ctx.eval_condition(&cc.condition) {
            note(cc.class.as_str());
        }
    }
    if ox || oy {
        Some((ox, oy))
    } else {
        None
    }
}

fn wrap_overflow_on_div(d: gpui::Div, ox: bool, oy: bool) -> gpui::Stateful<gpui::Div> {
    let n = SCROLL_OVERFLOW_ID.fetch_add(1, Ordering::Relaxed);
    let mut s = d.id(ElementId::Name(SharedString::from(format!(
        "crepus-scroll-{n}"
    ))));
    if ox {
        s = s.overflow_x_scroll();
    }
    if oy {
        s = s.overflow_y_scroll();
    }
    s
}

fn render_element<T: CrepusMouseDispatch + 'static>(
    el: &Element,
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    // Intercept the `slot` pseudo-tag: render slot content from parent, or fallback children.
    if el.tag == "slot" {
        return if let Some((slot_nodes, slot_ctx)) = &ctx.slot {
            render_nodes_maybe_dispatch(slot_nodes, slot_ctx, ix)
        } else {
            render_nodes_maybe_dispatch(&el.children, ctx, ix)
        };
    }

    let mut d = base_tag_element(&el.tag);

    // Apply static and dynamic classes with context for expression resolution
    for class in &el.classes {
        if is_overflow_scroll_class(class) {
            continue;
        }
        d = apply_class_with_ctx(d, class, Some(ctx));
    }

    // Apply conditional classes
    for cc in &el.conditional_classes {
        if ctx.eval_condition(&cc.condition) {
            if is_overflow_scroll_class(&cc.class) {
                continue;
            }
            d = apply_class_with_ctx(d, &cc.class, Some(ctx));
        }
    }

    // Render children
    for child in &el.children {
        let child_el = render_node_impl(child, ctx, ix);
        d = d.child(child_el);
    }

    let axes = scroll_axes(el, ctx);

    // Animations are only applied to plain `Div` builds; scroll uses `Stateful<Div>` (incompatible).
    if !el.animations.is_empty() {
        let d = attach_interactive_handlers(d, &el.event_handlers, ix);
        return match axes {
            None => render_with_animations(d, &el.animations, &el.tag),
            Some((ox, oy)) => wrap_overflow_on_div(d, ox, oy).into_any_element(),
        };
    }

    match axes {
        None => {
            let d = attach_interactive_handlers(d, &el.event_handlers, ix);
            d.into_any_element()
        }
        Some((ox, oy)) => {
            let d = attach_interactive_handlers(d, &el.event_handlers, ix);
            wrap_overflow_on_div(d, ox, oy).into_any_element()
        }
    }
}

/// Wrap a div with GPUI animations based on the parsed animation specs.
fn render_with_animations(d: gpui::Div, animations: &[AnimationSpec], tag: &str) -> AnyElement {
    // Generate a stable element ID from the tag + animation properties
    let props: Vec<&str> = animations.iter().map(|a| a.property.as_str()).collect();
    let id_str = format!("crepus-anim-{}-{}", tag, props.join("-"));
    let id = ElementId::Name(SharedString::from(id_str));

    if animations.len() == 1 {
        let spec = &animations[0];
        let duration_ms = parse_duration_ms(&spec.duration_expr).unwrap_or(300);
        let duration = Duration::from_millis(duration_ms);

        let mut anim = Animation::new(duration);
        anim = apply_easing(anim, &spec.easing);
        if spec.repeat {
            anim = anim.repeat();
        }

        let property = spec.property.clone();
        d.with_animation(id, anim, move |el, delta| {
            apply_animation_property(el, &property, delta)
        })
        .into_any_element()
    } else {
        let anims: Vec<Animation> = animations
            .iter()
            .map(|spec| {
                let duration_ms = parse_duration_ms(&spec.duration_expr).unwrap_or(300);
                let mut anim = Animation::new(Duration::from_millis(duration_ms));
                anim = apply_easing(anim, &spec.easing);
                if spec.repeat {
                    anim = anim.repeat();
                }
                anim
            })
            .collect();

        let properties: Vec<String> = animations.iter().map(|a| a.property.clone()).collect();
        d.with_animations(id, anims, move |el, ix, delta| {
            if ix < properties.len() {
                apply_animation_property(el, &properties[ix], delta)
            } else {
                el
            }
        })
        .into_any_element()
    }
}

fn apply_easing(anim: Animation, easing: &str) -> Animation {
    match easing {
        "linear" => anim.with_easing(linear),
        "ease-in-out" => anim.with_easing(ease_in_out),
        "quadratic" => anim.with_easing(quadratic),
        "bounce" => anim.with_easing(bounce(quadratic)),
        "ease-out" => anim.with_easing(ease_out_quint()),
        _ => anim, // default: linear
    }
}

/// Apply an animation delta (0.0 - 1.0) to a specific property on a div.
fn apply_animation_property(d: gpui::Div, property: &str, delta: f32) -> gpui::Div {
    match property {
        "opacity" | "fade" | "fade-in" => d.opacity(delta),
        "fade-out" => d.opacity(1.0 - delta),
        "pulse" => d.opacity(0.4 + delta * 0.6),
        "scale" => {
            // Scale from 0.8 to 1.0
            let scale = 0.8 + delta * 0.2;
            d.opacity(scale) // GPUI doesn't have transform: scale; use opacity as approximation
        }
        "slide-down" => {
            // Slide from -10px to 0px
            let offset = gpui::px(-10.0 * (1.0 - delta));
            d.mt(offset)
        }
        "slide-up" => {
            let offset = gpui::px(10.0 * (1.0 - delta));
            d.mt(offset)
        }
        "slide-right" => {
            let offset = gpui::px(-20.0 * (1.0 - delta));
            d.ml(offset)
        }
        "slide-left" => {
            let offset = gpui::px(20.0 * (1.0 - delta));
            d.ml(offset)
        }
        "grow" => {
            // Grow from w-0 to full
            let pct = gpui::relative(delta);
            d.w(pct)
        }
        _ => d,
    }
}

fn base_tag_element(tag: &str) -> gpui::Div {
    match tag {
        "button" => div().cursor_pointer(),
        _ => div(),
    }
}

fn render_text(parts: &[TextPart], ctx: &TemplateContext) -> String {
    let mut result = String::new();
    for part in parts {
        match part {
            TextPart::Literal(text) => result.push_str(text),
            TextPart::Expr(expr) => {
                let val = crate::eval::eval_expr(expr, ctx);
                result.push_str(&value_to_str(&val));
            }
        }
    }
    result
}

fn render_if<T: CrepusMouseDispatch + 'static>(
    block: &IfBlock,
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    if ctx.eval_condition(&block.condition) {
        render_nodes_maybe_dispatch(&block.then_children, ctx, ix)
    } else if let Some(else_children) = &block.else_children {
        render_nodes_maybe_dispatch(else_children, ctx, ix)
    } else {
        div().into_any_element()
    }
}

fn render_for<T: CrepusMouseDispatch + 'static>(
    block: &ForBlock,
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    let items = ctx.get_list(&block.iterator);

    let mut d = div();
    for item_ctx in items {
        let mut child_ctx = ctx.clone();
        for (k, v) in &item_ctx.vars {
            child_ctx.vars.insert(k.clone(), v.clone());
        }
        let pattern = block.pattern.trim();
        if !pattern.is_empty() {
            let item_str = item_ctx.get_str("value");
            if !item_str.is_empty() {
                child_ctx
                    .vars
                    .insert(pattern.to_string(), TemplateValue::Str(item_str));
            }
        }

        let child = render_nodes_maybe_dispatch(&block.body, &child_ctx, ix);
        d = d.child(child);
    }
    d.into_any_element()
}

fn render_match<T: CrepusMouseDispatch + 'static>(
    block: &MatchBlock,
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    let val = crate::eval::eval_expr(&block.expr, ctx);
    let value = value_to_str(&val);

    for arm in &block.arms {
        let pattern = arm.pattern.trim();
        if pattern == "_" {
            return render_nodes_maybe_dispatch(&arm.body, ctx, ix);
        }
        if pattern.starts_with('"') && pattern.ends_with('"') {
            let lit = &pattern[1..pattern.len() - 1];
            if value == lit {
                return render_nodes_maybe_dispatch(&arm.body, ctx, ix);
            }
        }
        if value == pattern {
            return render_nodes_maybe_dispatch(&arm.body, ctx, ix);
        }
    }

    div().into_any_element()
}

fn render_include<T: CrepusMouseDispatch + 'static>(
    inc: &IncludeNode,
    ctx: &TemplateContext,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    // Multi-component syntax: "path/file.crepus#ComponentName"
    if let Some((file_part, comp_name)) = inc.path.split_once('#') {
        return render_named_component(inc, ctx, file_part, comp_name, ix);
    }

    // Single-component file: resolve path relative to the current file's directory.
    let file_path = resolve_include_path(ctx.base_dir.as_deref(), &inc.path);

    let content = match read_crepus_source(ctx, &file_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("include error: {:?}: {}", file_path, e);
            return div()
                .text_color(rgb(0xff4444))
                .child(SharedString::from(msg))
                .into_any_element();
        }
    };

    let nodes = match crate::parser::parse_template(&content) {
        Ok(n) => n,
        Err(e) => {
            let msg = format!("include parse error: {}", e);
            return div()
                .text_color(rgb(0xff4444))
                .child(SharedString::from(msg))
                .into_any_element();
        }
    };

    // Build child context: fresh vars from evaluated props, correct base_dir, and slot.
    let mut child_ctx = TemplateContext::new();
    child_ctx.base_dir = file_path.parent().map(|p| p.to_path_buf());
    child_ctx.virtual_files.clone_from(&ctx.virtual_files);

    for (key, expr) in &inc.props {
        let val = crate::eval::eval_expr(expr, ctx);
        child_ctx.vars.insert(key.clone(), val);
    }

    if !inc.slot.is_empty() {
        child_ctx.slot = Some((inc.slot.clone(), Box::new(ctx.clone())));
    }

    render_nodes_maybe_dispatch(&nodes, &child_ctx, ix)
}

fn resolve_include_path(base_dir: Option<&std::path::Path>, path: &str) -> PathBuf {
    let candidate = if let Some(base) = base_dir {
        base.join(path)
    } else {
        PathBuf::from(path)
    };

    std::fs::canonicalize(&candidate).unwrap_or(candidate)
}

/// Render a named component from a multi-component file (`path#Name` syntax).
fn render_named_component<T: CrepusMouseDispatch + 'static>(
    inc: &IncludeNode,
    ctx: &TemplateContext,
    file_part: &str,
    comp_name: &str,
    ix: Option<&WeakEntity<T>>,
) -> AnyElement {
    let file_path = resolve_include_path(ctx.base_dir.as_deref(), file_part);

    let content = match read_crepus_source(ctx, &file_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("include error: {:?}: {}", file_path, e);
            return div()
                .text_color(rgb(0xff4444))
                .child(SharedString::from(msg))
                .into_any_element();
        }
    };

    let comp_file = match crate::parser::parse_component_file(&content) {
        Ok(cf) => cf,
        Err(e) => {
            let msg = format!("component file parse error: {}", e);
            return div()
                .text_color(rgb(0xff4444))
                .child(SharedString::from(msg))
                .into_any_element();
        }
    };

    let comp = match comp_file.components.get(comp_name) {
        Some(c) => c,
        None => {
            let mut keys: Vec<&str> = comp_file.components.keys().map(|s| s.as_str()).collect();
            keys.sort();
            let msg = format!(
                "component '{}' not found in {}; available: [{}]",
                comp_name,
                file_part,
                keys.join(", ")
            );
            return div()
                .text_color(rgb(0xff4444))
                .child(SharedString::from(msg))
                .into_any_element();
        }
    };

    let mut child_ctx = TemplateContext::new();
    child_ctx.base_dir = file_path.parent().map(|p| p.to_path_buf());
    child_ctx.virtual_files.clone_from(&ctx.virtual_files);

    // Inject TOML defaults first — passed props override them.
    for (key, expr) in &comp.meta.defaults {
        let val = crate::eval::eval_expr(expr, &TemplateContext::new());
        child_ctx.vars.insert(key.clone(), val);
    }

    // Apply passed props.
    for (key, expr) in &inc.props {
        let val = crate::eval::eval_expr(expr, ctx);
        child_ctx.vars.insert(key.clone(), val);
    }

    if !inc.slot.is_empty() {
        child_ctx.slot = Some((inc.slot.clone(), Box::new(ctx.clone())));
    }

    render_nodes_maybe_dispatch(&comp.nodes, &child_ctx, ix)
}
