use std::ffi::{CStr, CString, c_char, c_void};
pub type W = *mut c_void;
pub const NULL: W = std::ptr::null_mut();
pub fn c(value: &str) -> CString {
    CString::new(value.replace('\0', "�")).unwrap()
}

unsafe extern "C" {
    pub fn sl_init();
    pub fn sl_exec() -> i32;
    pub fn sl_quit();
    pub fn sl_timer();
    pub fn sl_timer_mode(milliseconds: i32);
    pub fn sl_wake();
    pub fn sl_palette_begin();
    pub fn sl_palette_end();
    pub fn sl_map_fill(p: W, x: f64, y: f64, w: f64, h: f64, color: i32, dark: i32);
    pub fn sl_font_key(p: W) -> u64;
    pub fn sl_text_run_new(
        name: *const u8,
        name_size: usize,
        value: *const u8,
        value_size: usize,
    ) -> W;
    pub fn sl_text_run_free(run: W);
    pub fn sl_text_run_width(p: W, run: W) -> f64;
    pub fn sl_text_run_draw(
        p: W,
        run: W,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        secondary: i32,
        dark: i32,
    );
    pub fn sl_style(css: *const c_char, scale: f64);
    pub fn sl_palette(role: i32, group: i32, color: *const c_char);
    pub fn sl_icon(bytes: *const u8, size: i32);
    pub fn sl_app_dark(dark: i32);
    pub fn sl_local_time(seconds: u64, buffer: *mut c_char);
    pub fn sl_is_visible(widget: W) -> i32;
    pub fn sl_opacity(widget: W) -> f64;
    pub fn sl_new(kind: i32, parent: W, name: *const c_char, id: i32) -> W;
    pub fn sl_destroy(widget: W);
    pub fn sl_central(window: W, child: W);
    pub fn sl_layout(horizontal: i32, parent: W, margin: i32, spacing: i32) -> W;
    pub fn sl_margins(layout: W, l: i32, t: i32, r: i32, b: i32);
    pub fn sl_add(layout: W, widget: W, stretch: i32);
    pub fn sl_add_layout(layout: W, child: W, stretch: i32);
    pub fn sl_stretch(layout: W, stretch: i32);
    pub fn sl_add_center(layout: W, widget: W);
    pub fn sl_cursor(widget: W, closed: i32);
    pub fn sl_split_add(split: W, child: W);
    pub fn sl_split_sizes(split: W, a: i32, b: i32);
    pub fn sl_scroll_child(scroll: W, child: W);
    pub fn sl_split_stretch(split: W, index: i32, factor: i32);
    pub fn sl_fixed(widget: W, width: i32, height: i32);
    pub fn sl_tab_add(tabs: W, child: W, title: *const c_char);
    pub fn sl_tab_clear(tabs: W);
    pub fn sl_stack_add(stack: W, child: W);
    pub fn sl_stack_set(stack: W, child: W);
    pub fn sl_set(object: W, name: *const c_char, value: *const c_char);
    pub fn sl_int(object: W, name: *const c_char, value: i32);
    pub fn sl_bool(object: W, name: *const c_char, value: i32);
    pub fn sl_wrap(widget: W, wrap: i32);
    pub fn sl_ignored(widget: W);
    pub fn sl_align(widget: W, alignment: i32);
    pub fn sl_show(widget: W, fullscreen: i32);
    pub fn sl_hide(widget: W);
    pub fn sl_focus(widget: W);
    pub fn sl_resize(widget: W, width: i32, height: i32);
    pub fn sl_dialog(widget: W, kind: i32);
    pub fn sl_popup(widget: W, anchor: W);
    pub fn sl_brand(widget: W, size: i32);
    pub fn sl_width(widget: W) -> i32;
    pub fn sl_height(widget: W) -> i32;
    pub fn sl_hint_width(widget: W) -> i32;
    pub fn sl_hwnd(widget: W) -> i64;
    pub fn sl_group(parent: W, buttons: *const W, count: i32);
    pub fn sl_navigation_icon(widget: W, direction: i32, scale: f64);
    pub fn sl_view(widget: W, id: i32, event: i32, scale: f64);
    pub fn sl_model_reset(widget: W);
    pub fn sl_row(widget: W) -> i32;
    pub fn sl_select_row(widget: W, row: i32);
    pub fn sl_tree_expand(widget: W, node: u64, expanded: i32);
    pub fn sl_tree_select(widget: W, node: u64);
    pub fn sl_collapse(widget: W);
    pub fn sl_action(
        widget: W,
        name: *const c_char,
        shortcut: *const c_char,
        id: i32,
        separator: i32,
    ) -> W;
    pub fn sl_shortcut(widget: W, key: *const c_char, id: i32);
    pub fn sl_choose(widget: W) -> *mut c_char;
    pub fn sl_cpp_free(value: *mut c_char);
    pub fn sl_clipboard(value: *const c_char);
    pub fn sl_message(widget: W, title: *const c_char, message: *const c_char);
    pub fn sl_shortcuts(widget: W, scale: f64);
    pub fn sl_tooltip(widget: W, x: f64, y: f64, value: *const c_char);
    pub fn sl_update(widget: W);
    pub fn sl_range(widget: W, minimum: i32, maximum: i32, value: i32);
    pub fn sl_progress(milliseconds: i32);
    pub fn sl_fade(widget: W, from: f64, to: f64, duration: i32);
    pub fn sl_present(widget: W, animated: i32);
    pub fn sl_snapshot(widget: W, duration: i32) -> W;
    pub fn sl_exit(widget: W, duration: i32);
    pub fn sl_outline(
        p: W,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: *const c_char,
        radius: f64,
        width: f64,
    );
    pub fn sl_fill(p: W, x: f64, y: f64, w: f64, h: f64, color: *const c_char, radius: f64);
    pub fn sl_save(p: W);
    pub fn sl_restore(p: W);
    pub fn sl_clip(p: W, x: f64, y: f64, w: f64, h: f64);
    pub fn sl_translate(p: W, x: f64, y: f64, angle: f64);
    pub fn sl_text_width(p: W, value: *const c_char) -> f64;
    pub fn sl_line_height(p: W) -> f64;
    pub fn sl_text(
        p: W,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        value: *const c_char,
        color: *const c_char,
        right: i32,
        elide: i32,
    );
    pub fn sl_font(p: W, pixels: f64, bold: i32);
    pub fn sl_line(p: W, x1: f64, y1: f64, x2: f64, y2: f64, color: *const c_char, width: f64);
}

pub fn widget(kind: i32, parent: W, name: &str, event: i32) -> W {
    unsafe { sl_new(kind, parent, c(name).as_ptr(), event) }
}
pub fn set(w: W, key: &str, value: &str) {
    unsafe {
        sl_set(w, c(key).as_ptr(), c(value).as_ptr());
    }
}
pub fn int(w: W, key: &str, value: i32) {
    unsafe {
        sl_int(w, c(key).as_ptr(), value);
    }
}
pub fn boolean(w: W, key: &str, value: bool) {
    unsafe {
        sl_bool(w, c(key).as_ptr(), value as i32);
    }
}
pub fn text(w: W, value: &str) {
    set(w, "text", value);
}
pub fn enabled(w: W, value: bool) {
    boolean(w, "enabled", value);
}
pub fn visible(w: W, value: bool) {
    boolean(w, "visible", value);
}
pub fn show(w: W) {
    unsafe {
        sl_show(w, 0);
    }
}
pub fn label(parent: W, value: &str, name: &str, wrap: bool) -> W {
    let w = widget(1, parent, name, 0);
    text(w, value);
    unsafe {
        sl_wrap(w, wrap as i32);
    }
    w
}
pub fn button(parent: W, value: &str, name: &str, event: i32) -> W {
    let w = widget(2, parent, name, event);
    text(w, value);
    w
}
pub fn add(layout: W, widget: W, stretch: i32) {
    unsafe {
        sl_add(layout, widget, stretch);
    }
}
pub fn layout(parent: W, horizontal: bool, margin: i32, spacing: i32) -> W {
    unsafe { sl_layout(horizontal as i32, parent, margin, spacing) }
}
pub fn row(parent: W) -> W {
    layout(parent, true, 0, 8)
}
pub fn column(parent: W) -> W {
    layout(parent, false, 0, 16)
}
pub fn sub(layout: W, child: W, stretch: i32) {
    unsafe {
        sl_add_layout(layout, child, stretch);
    }
}
pub fn stretch(layout: W) {
    unsafe {
        sl_stretch(layout, 1);
    }
}
pub fn margins(layout: W, l: i32, t: i32, r: i32, b: i32) {
    unsafe {
        sl_margins(layout, l, t, r, b);
    }
}
pub fn shortcut(w: W, key: &str, id: i32) {
    unsafe {
        sl_shortcut(w, c(key).as_ptr(), id);
    }
}
pub fn choose(w: W) -> Option<std::path::PathBuf> {
    unsafe {
        let value = sl_choose(w);
        let result = CStr::from_ptr(value).to_string_lossy().into_owned();
        sl_cpp_free(value);
        if result.is_empty() {
            None
        } else {
            Some(result.into())
        }
    }
}
pub fn message(w: W, title: &str, message: &str) {
    unsafe {
        sl_message(w, c(title).as_ptr(), c(message).as_ptr());
    }
}
pub fn fill(p: W, r: crate::geometry::Rect, color: &str, radius: f64) {
    unsafe {
        sl_fill(p, r.x, r.y, r.w, r.h, c(color).as_ptr(), radius);
    }
}
pub fn draw_text(
    p: W,
    r: crate::geometry::Rect,
    value: &str,
    color: &str,
    right: bool,
    elide: i32,
) {
    unsafe {
        sl_text(
            p,
            r.x,
            r.y,
            r.w,
            r.h,
            c(value).as_ptr(),
            c(color).as_ptr(),
            right as i32,
            elide,
        );
    }
}
