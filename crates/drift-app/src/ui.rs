#![allow(dead_code)]

use std::ops::Range;
use std::process::Command;

use anyhow::{Context as _, Result};
use crepuscularity_gpui::prelude::*;
use gpui::{
    actions, bounds, div, fill, hsla, point, rgba, size, App as GpuiApp, Application, Bounds,
    ClipboardItem, CursorStyle, Element, ElementId, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, Style,
    StatefulInteractiveElement, TextRun, UTF16Selection, UnderlineStyle, WindowBounds, WindowOptions,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{cli, config::AppConfig};

actions!(
    drift_text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Quit,
    ]
);

pub fn run_ui(initial_config: AppConfig) -> Result<()> {
    Application::new().run(move |cx: &mut GpuiApp| {
        bind_text_input_keys(cx);

        let bounds = bounds(point(px(72.), px(72.)), size(px(720.), px(520.)));
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: gpui::WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            display_id: None,
            window_background: gpui::WindowBackgroundAppearance::Opaque,
            app_id: Some("drift-wallpaper.controls".to_string()),
            window_min_size: Some(size(px(520.), px(380.))),
            window_decorations: None,
            tabbing_identifier: None,
        };

        let window = cx
            .open_window(window_options, move |_window, cx| {
                cx.new(|cx| DriftUi::new(initial_config.clone(), cx))
            })
            .expect("open gpui controls window");

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.custom_colors.focus_handle(cx));
                cx.activate(true);
            })
            .ok();

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    });

    Ok(())
}

fn bind_text_input_keys(cx: &mut GpuiApp) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
    ]);
}

struct DriftUi {
    config: AppConfig,
    status: SharedString,
    custom_colors: Entity<TextInput>,
    image_path: Entity<TextInput>,
}

impl DriftUi {
    fn new(config: AppConfig, cx: &mut Context<Self>) -> Self {
        let colors = colors_as_hex_triplet(&config);
        Self {
            config,
            status: "Ready".into(),
            custom_colors: cx.new(|cx| TextInput::new(colors.into(), "hex colors".into(), cx)),
            image_path: cx
                .new(|cx| TextInput::new("".into(), "image or screenshot path".into(), cx)),
        }
    }

    fn sync_custom_field(&self, cx: &mut Context<Self>) {
        let colors = colors_as_hex_triplet(&self.config);
        self.custom_colors
            .update(cx, |input, cx| input.set_text(colors.into(), cx));
    }

    fn save_config(&mut self) -> Result<()> {
        self.config.save()
    }

    fn set_status_ok(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = message.into();
        cx.notify();
    }

    fn set_status_err(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.status = message.into().into();
        cx.notify();
    }

    fn apply_preset_name(&mut self, preset: &str, cx: &mut Context<Self>) {
        match cli::apply_named_preset(&mut self.config, preset).and_then(|_| self.save_config()) {
            Ok(()) => {
                self.sync_custom_field(cx);
                self.set_status_ok(format!("Applied {preset} preset"), cx);
            }
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn apply_ocean(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_preset_name("ocean", cx);
    }

    fn apply_sunset(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_preset_name("sunset", cx);
    }

    fn apply_forest(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_preset_name("forest", cx);
    }

    fn apply_lava(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_preset_name("lava", cx);
    }

    fn apply_midnight(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_preset_name("midnight", cx);
    }

    fn apply_monochrome(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_preset_name("monochrome", cx);
    }

    fn apply_custom(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.custom_colors.read(cx).text();
        match cli::apply_custom_colors(&mut self.config, &text).and_then(|_| self.save_config()) {
            Ok(()) => {
                self.sync_custom_field(cx);
                self.set_status_ok("Applied custom colors", cx);
            }
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn apply_image_path(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let path = self.image_path.read(cx).text();
        match cli::apply_image_palette(&mut self.config, std::path::Path::new(&path))
            .and_then(|_| self.save_config())
        {
            Ok(()) => {
                self.sync_custom_field(cx);
                self.set_status_ok(format!("Extracted colors from {path}"), cx);
            }
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn apply_current_wallpaper(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match cli::apply_current_wallpaper_palette(&mut self.config)
            .and_then(|_| self.save_config())
        {
            Ok(()) => {
                self.sync_custom_field(cx);
                self.set_status_ok("Matched current wallpaper image", cx);
            }
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn apply_wallpaper_screenshot(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match cli::apply_wallpaper_screenshot_palette(&mut self.config)
            .and_then(|_| self.save_config())
        {
            Ok(()) => {
                self.sync_custom_field(cx);
                self.set_status_ok("Matched current wallpaper screenshot", cx);
            }
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn open_background(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        match spawn_mode("--background") {
            Ok(()) => self.set_status_ok("Launched background renderer", cx),
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn open_preview(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        match spawn_mode("--preview") {
            Ok(()) => self.set_status_ok("Launched preview renderer", cx),
            Err(error) => self.set_status_err(error.to_string(), cx),
        }
    }

    fn toggle_wallpaper_enabled(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.config.enabled = !self.config.enabled;
        match self.save_config() {
            Ok(()) => {
                let msg = if self.config.enabled {
                    "Wallpaper rendering enabled"
                } else {
                    "Wallpaper rendering paused (config saved)"
                };
                self.set_status_ok(msg, cx);
            }
            Err(error) => {
                self.config.enabled = !self.config.enabled;
                self.set_status_err(error.to_string(), cx);
            }
        }
    }
}

impl Render for DriftUi {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.status.clone();
        let swatches = current_swatches(&self.config);
        let custom_colors = self.custom_colors.clone();
        let image_path = self.image_path.clone();

        let wallpaper_on = self.config.enabled;
        let toggle_label: SharedString = if wallpaper_on {
            "Wallpaper: on (click to pause)".into()
        } else {
            "Wallpaper: off (click to enable)".into()
        };

        let body = view! {r#"
            div min-h-full grid grid-cols-2 gap-0

                div p-6 flex flex-col gap-5 border-r border-zinc-900
                    div flex flex-col gap-2
                        div text-xs uppercase tracking-[0.2em] text-zinc-500
                            "Current palette"
                        {swatches}

                    div flex flex-col gap-2
                        div text-xs uppercase tracking-[0.2em] text-zinc-500
                            "Presets"
                        div flex flex-wrap gap-2
                            {action_button("Ocean", cx.listener(Self::apply_ocean))}
                            {action_button("Sunset", cx.listener(Self::apply_sunset))}
                            {action_button("Forest", cx.listener(Self::apply_forest))}
                            {action_button("Lava", cx.listener(Self::apply_lava))}
                            {action_button("Midnight", cx.listener(Self::apply_midnight))}
                            {action_button("Monochrome", cx.listener(Self::apply_monochrome))}

                    div flex flex-col gap-2
                        div text-xs uppercase tracking-[0.2em] text-zinc-500
                            "Custom colors"
                        div text-xs text-zinc-400
                            "Three hex colors, comma-separated."
                        {custom_colors}
                        div flex gap-2 flex-wrap
                            {primary_button("Apply custom", cx.listener(Self::apply_custom))}

                div p-6 flex flex-col gap-5
                    div flex flex-col gap-2
                        div text-xs uppercase tracking-[0.2em] text-zinc-500
                            "Reference image"
                        div text-xs text-zinc-400
                            "Image path, then extract palette."
                        {image_path}
                        div flex gap-2 flex-wrap
                            {primary_button("Extract from path", cx.listener(Self::apply_image_path))}
                            {action_button("Current wallpaper", cx.listener(Self::apply_current_wallpaper))}
                            {action_button("Wallpaper screenshot", cx.listener(Self::apply_wallpaper_screenshot))}

                    div flex flex-col gap-2
                        div text-xs uppercase tracking-[0.2em] text-zinc-500
                            "Renderer"
                        div text-xs text-zinc-400
                            "Background or preview; uses saved config."
                        div flex gap-2 flex-wrap
                            {primary_button("Launch background", cx.listener(Self::open_background))}
                            {action_button("Launch preview", cx.listener(Self::open_preview))}
        "#};

        let scroll = div()
            .id("drift-controls-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(body);

        view! {r#"
            div w-full h-full min-h-0 flex flex-col bg-zinc-950 text-white font-['Instrument_Sans']

                div px-6 pt-5 pb-4 flex flex-col gap-2 border-b border-zinc-800 shrink-0
                    div text-3xl font-bold tracking-[-0.03em]
                        "Drift Wallpaper"
                    div text-sm text-zinc-400 max-w-[640px] leading-snug
                        "Palettes, wallpaper colors, and renderer launch."
                    div flex flex-wrap items-center gap-2
                        {action_button(toggle_label, cx.listener(Self::toggle_wallpaper_enabled))}
                    div text-xs text-emerald-300
                        "{status}"

                { scroll }
        "#}
    }
}

fn spawn_mode(flag: &str) -> Result<()> {
    let exe = std::env::current_exe().context("Resolve current executable")?;
    Command::new(exe)
        .arg(flag)
        .spawn()
        .with_context(|| format!("Spawn renderer with {flag}"))?;
    Ok(())
}

fn current_swatches(config: &AppConfig) -> impl IntoElement {
    let colors = [
        color_to_hex(config.params.color_a),
        color_to_hex(config.params.color_b),
        color_to_hex(config.params.color_c),
    ];
    let fills = [
        color_to_rgba_u32(config.params.color_a),
        color_to_rgba_u32(config.params.color_b),
        color_to_rgba_u32(config.params.color_c),
    ];

    div().flex().gap_3().children((0..3).map(move |index| {
        div()
            .w(px(120.))
            .flex()
            .flex_col()
            .gap_1()
            .child(div().h(px(72.)).rounded_md().bg(rgba(fills[index])))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xe4e4e7))
                    .child(colors[index].clone()),
            )
    }))
}

fn colors_as_hex_triplet(config: &AppConfig) -> String {
    format!(
        "{},{},{}",
        color_to_hex(config.params.color_a),
        color_to_hex(config.params.color_b),
        color_to_hex(config.params.color_c)
    )
}

fn color_to_hex(color: [f32; 3]) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8
    )
}

fn color_to_rgba_u32(color: [f32; 3]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0).round() as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0).round() as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0).round() as u32;
    (r << 24) | (g << 16) | (b << 8) | 0xff
}

fn action_button(
    label: impl Into<SharedString>,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut GpuiApp) + 'static,
) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(rgb(0x18181b))
        .border_1()
        .border_color(rgb(0x27272a))
        .text_sm()
        .text_color(rgb(0xf4f4f5))
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(rgb(0x27272a)))
        .on_mouse_up(MouseButton::Left, listener)
        .child(label.into())
}

fn primary_button(
    label: impl Into<SharedString>,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut GpuiApp) + 'static,
) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(rgb(0x2563eb))
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(white())
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(rgb(0x1d4ed8)))
        .on_mouse_up(MouseButton::Left, listener)
        .child(label.into())
}

struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    fn new(content: SharedString, placeholder: SharedString, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content,
            placeholder,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    fn text(&self) -> String {
        self.content.to_string()
    }

    fn set_text(&mut self, text: SharedString, cx: &mut Context<Self>) {
        self.content = text;
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _cx: &mut GpuiApp,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], _cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut GpuiApp,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let (display_text, text_color): (SharedString, _) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0.7, 0.45))
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x3311ff30),
                )),
                None,
            )
        };

        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut GpuiApp,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(0x09090b))
                    .border_1()
                    .border_color(rgb(0x27272a))
                    .text_color(white())
                    .text_sm()
                    .line_height(px(24.))
                    .child(TextElement { input: cx.entity() }),
            )
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus_handle.clone()
    }
}
