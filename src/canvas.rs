use crate::{
    geometry::{Layout, Rect, spring_step, vertical_label},
    qt::{self, W},
    scanner::{Entry, size_text},
    view::EntryView,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    time::Instant,
};

pub struct Canvas {
    pub entries: EntryView,
    pub layout: Layout,
    visible: Vec<usize>,
    labels: HashMap<usize, TextRun>,
    font_key: u64,
    pub zoom: f64,
    pub target: f64,
    pub velocity: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub anchor: (f64, f64),
    pub scene: (f64, f64),
    press: Option<(f64, f64, f64, f64)>,
    pending_click: bool,
    dragging: bool,
    angles: Vec<bool>,
    pub widget: W,
    pub message: String,
    incoming: Option<(Rect, Instant)>,
    last: Instant,
    phase: f64,
    zoom_limit: f64,
    item_focus: Option<Rect>,
    wheel_focus: Option<(Rect, (f64, f64), Instant)>,
    focus: Option<((f64, f64, f64), (f64, f64, f64), f64, f64)>,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            entries: EntryView::default(),
            layout: Layout::default(),
            visible: Vec::new(),
            labels: HashMap::new(),
            font_key: 0,
            zoom: 1.,
            target: 1.,
            velocity: 0.,
            x: 0.,
            y: 0.,
            width: 0.,
            height: 0.,
            anchor: (0., 0.),
            scene: (0., 0.),
            press: None,
            pending_click: false,
            dragging: false,
            angles: vec![],
            widget: qt::NULL,
            message: "Choose a folder to build its space map.".into(),
            incoming: None,
            last: Instant::now(),
            phase: 0.,
            zoom_limit: 12.,
            item_focus: None,
            wheel_focus: None,
            focus: None,
        }
    }
}
thread_local! {
    pub static MAPS:RefCell<[Canvas;4]>=RefCell::new(std::array::from_fn(|_|Canvas::default()));
    static DARK:Cell<bool>=const{Cell::new(false)};
    static SCALE:Cell<f64>=const{Cell::new(1.)};
    static REDUCED:Cell<bool>=const{Cell::new(false)};
}
pub fn configure(dark: bool, scale: f64, reduced: bool) {
    DARK.set(dark);
    SCALE.set(scale);
    REDUCED.set(reduced);
}
pub fn bind(id: usize, widget: W) {
    MAPS.with(|maps| maps.borrow_mut()[id].widget = widget);
}
pub fn set_entries(id: usize, entries: EntryView, origin: Option<Rect>) {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        let map = &mut maps[id];
        map.entries = entries;
        map.angles = vec![false; map.entries.len()];
        map.layout.clear();
        map.labels.clear();
        map.reset();
        map.incoming = if REDUCED.get() {
            None
        } else {
            origin.map(|r| (r, Instant::now()))
        };
        if !map.widget.is_null() {
            unsafe {
                qt::sl_update(map.widget);
            }
        }
    });
}
struct TextRun(W);
impl Drop for TextRun {
    fn drop(&mut self) {
        unsafe {
            qt::sl_text_run_free(self.0);
        }
    }
}
#[derive(Clone, Copy)]
struct Transform {
    sx: f64,
    sy: f64,
    x: f64,
    y: f64,
}
impl Transform {
    fn rect(self, r: Rect) -> Rect {
        Rect {
            x: r.x * self.sx + self.x,
            y: r.y * self.sy + self.y,
            w: r.w * self.sx,
            h: r.h * self.sy,
        }
    }
    fn scene(self, r: Rect) -> Rect {
        Rect {
            x: (r.x - self.x) / self.sx,
            y: (r.y - self.y) / self.sy,
            w: r.w / self.sx,
            h: r.h / self.sy,
        }
    }
}
impl Canvas {
    fn moving(&self) -> bool {
        (self.zoom - self.target).abs() > 0.000001 || self.velocity.abs() > 0.000001
    }
    fn reset(&mut self) {
        self.zoom = 1.;
        self.target = 1.;
        self.velocity = 0.;
        self.x = 0.;
        self.y = 0.;
        self.press = None;
        self.pending_click = false;
        self.focus = None;
        self.wheel_focus = None;
        self.item_focus = None;
    }
    fn clamp(&mut self) {
        self.x = self.x.max(self.width * (1. - self.zoom)).min(0.);
        self.y = self.y.max(self.height * (1. - self.zoom)).min(0.);
    }
    fn zoom_to(&mut self, value: f64, anchor: (f64, f64), animated: bool) {
        if self.entries.is_empty() {
            return;
        }
        self.focus = None;
        let was_moving = self.moving();
        self.anchor = anchor;
        self.scene = (
            (anchor.0 - self.x) / self.zoom,
            (anchor.1 - self.y) / self.zoom,
        );
        self.target = value.clamp(1., self.zoom_limit);
        // Retargeting must preserve elapsed time as well as spring velocity.
        if !was_moving {
            self.last = Instant::now();
        }
        if !animated || REDUCED.get() {
            self.zoom = self.target;
            self.velocity = 0.;
            self.x = anchor.0 - self.scene.0 * self.zoom;
            self.y = anchor.1 - self.scene.1 * self.zoom;
            self.clamp();
        }
    }
    fn item_anchor(&self, r: Rect) -> (f64, f64) {
        (
            ((r.x + r.w / 2.) * self.zoom + self.x).clamp(0., self.width),
            ((r.y + r.h / 2.) * self.zoom + self.y).clamp(0., self.height),
        )
    }
    fn nearby(&self, point: (f64, f64)) -> Option<Rect> {
        self.layout.nearby_tiny(
            point,
            self.zoom,
            (self.x, self.y),
            36. * SCALE.get(),
            Rect {
                x: 0.,
                y: 0.,
                w: self.width,
                h: self.height,
            },
        )
    }
    fn focus_at(&mut self, point: (f64, f64)) {
        let Some(rect) = self.nearby(point).or_else(|| {
            self.layout
                .hit(
                    (point.0 - self.x) / self.zoom,
                    (point.1 - self.y) / self.zoom,
                )
                .map(|(_, r)| r)
        }) else {
            return;
        };
        let fit = (self.width / rect.w).min(self.height / rect.h) * 0.92;
        let z = fit
            .max(self.width.min(self.height) * 0.4 / rect.w.min(rect.h))
            .clamp(1., self.zoom_limit);
        let x = (self.width / 2. - (rect.x + rect.w / 2.) * z).clamp(self.width * (1. - z), 0.);
        let y = (self.height / 2. - (rect.y + rect.h / 2.) * z).clamp(self.height * (1. - z), 0.);
        self.wheel_focus = None;
        self.item_focus = Some(rect);
        self.velocity = 0.;
        if REDUCED.get() {
            self.zoom = z;
            self.target = z;
            self.x = x;
            self.y = y;
        } else {
            self.focus = Some(((self.zoom, self.x, self.y), (z, x, y), 0., 0.));
            self.last = Instant::now();
            self.target = self.zoom;
        }
    }
    fn transform(&self) -> Transform {
        let (mut sx, mut sy, mut x, mut y) = (self.zoom, self.zoom, self.x, self.y);
        if let Some((origin, start)) = self.incoming {
            let t = 1. - (1. - (start.elapsed().as_secs_f64() / 0.22).min(1.)).powi(3);
            let ax = origin.w / self.width.max(1.) * (1. - t) + t;
            let ay = origin.h / self.height.max(1.) * (1. - t) + t;
            sx *= ax;
            sy *= ay;
            x = origin.x * (1. - t) + x * ax;
            y = origin.y * (1. - t) + y * ay;
        }
        Transform { sx, sy, x, y }
    }
    fn hit(&self, x: f64, y: f64) -> Option<(Entry, Rect, Option<i32>)> {
        let t = self.transform();
        let (i, r) = self.layout.hit((x - t.x) / t.sx, (y - t.y) / t.sy)?;
        Some((
            self.entries[i].clone(),
            t.rect(r),
            self.entries.table_row(i),
        ))
    }
}
pub fn zoom(id: usize, factor: f64, reset: bool) {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        let map = &mut maps[id];
        let value = if reset {
            map.item_focus = None;
            map.wheel_focus = None;
            1.
        } else {
            map.target * factor
        };
        let anchor = map
            .item_focus
            .map(|r| map.item_anchor(r))
            .unwrap_or((map.width / 2., map.height / 2.));
        map.zoom_to(value, anchor, !reset);
        unsafe {
            qt::sl_update(map.widget);
        }
    });
}
pub fn zoom_value(id: usize) -> f64 {
    MAPS.with(|maps| maps.borrow()[id].zoom)
}
pub fn pause(id: usize) {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        let map = &mut maps[id];
        map.target = map.zoom;
        map.velocity = 0.;
        map.focus = None;
        map.wheel_focus = None;
        map.incoming = None;
    });
}
pub fn needs_animation() -> bool {
    MAPS.with(|maps| {
        maps.borrow()
            .iter()
            .take(2)
            .any(|m| m.moving() || m.focus.is_some() || m.incoming.is_some())
    })
}
pub fn loading_tick() {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        let map = &mut maps[3];
        if map.last.elapsed().as_millis() >= 40 {
            map.last = Instant::now();
            map.phase = (map.phase + 2.) % 100.;
            if !map.widget.is_null() {
                unsafe {
                    qt::sl_update(map.widget);
                }
            }
        }
    });
}
pub fn animate() -> bool {
    let mut changed = false;
    MAPS.with(|maps| {
        for map in maps.borrow_mut().iter_mut().take(2) {
            let moving = map.moving() || map.focus.is_some();
            if let Some((start, end, progress, velocity)) = map.focus {
                let now = Instant::now();
                let (t, v) = spring_step(
                    progress,
                    velocity,
                    1.,
                    now.duration_since(map.last).as_secs_f64().min(0.05),
                );
                map.last = now;
                let t = if t > 0.9999 { 1. } else { t };
                map.zoom = start.0 + (end.0 - start.0) * t;
                map.target = map.zoom;
                map.x = start.1 + (end.1 - start.1) * t;
                map.y = start.2 + (end.2 - start.2) * t;
                map.clamp();
                map.focus = if t == 1. {
                    None
                } else {
                    Some((start, end, t, v))
                };
                changed = true;
            } else if moving {
                let now = Instant::now();
                let dt = now.duration_since(map.last).as_secs_f64().min(0.05);
                map.last = now;
                (map.zoom, map.velocity) = spring_step(map.zoom, map.velocity, map.target, dt);
                if (map.zoom - map.target).abs() < (map.target * 1e-9).max(0.001)
                    && map.velocity.abs() < (map.target * 1e-8).max(0.01)
                {
                    map.zoom = map.target;
                    map.velocity = 0.;
                }
                map.x = map.anchor.0 - map.scene.0 * map.zoom;
                map.y = map.anchor.1 - map.scene.1 * map.zoom;
                map.clamp();
                changed = true;
            }
            let incoming = map
                .incoming
                .is_some_and(|(_, start)| start.elapsed().as_secs_f64() < 0.22);
            if !incoming {
                map.incoming = None;
            }
            if (moving || incoming) && !map.widget.is_null() {
                unsafe {
                    qt::sl_update(map.widget);
                }
            }
        }
    });
    changed
}
pub fn input(
    id: usize,
    kind: i64,
    button: i64,
    x: f64,
    y: f64,
    delta: f64,
) -> Option<(Entry, Option<Rect>, Option<i32>)> {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        let map = &mut maps[id];
        if id == 2 {
            if kind == 1 && button == 1 {
                let row = (y / (64. * SCALE.get())) as usize;
                let inside = y - row as f64 * 64. * SCALE.get() < 56. * SCALE.get();
                return map
                    .entries
                    .rows
                    .get(row)
                    .filter(|_| inside)
                    .map(|_| (map.entries[row].clone(), None, map.entries.table_row(row)));
            }
            return None;
        }
        if id > 1 {
            return None;
        }
        let mut repaint = false;
        match kind {
            0 if button == 1 => {
                map.pending_click = false;
                map.focus = None;
                map.wheel_focus = None;
                map.target = map.zoom;
                map.velocity = 0.;
                map.press = Some((x, y, map.x, map.y));
                map.dragging = false;
            }
            1 if button == 1 => {
                let click = map.pending_click || (map.press.take().is_some() && !map.dragging);
                map.pending_click = false;
                map.dragging = false;
                unsafe {
                    qt::sl_cursor(map.widget, 0);
                }
                if click {
                    return map.hit(x, y).map(|(e, r, row)| (e, Some(r), row));
                }
            }
            2 => {
                if let Some((px, py, ox, oy)) = map.press {
                    if map.dragging || (x - px).abs() + (y - py).abs() > 6. {
                        map.dragging = true;
                        map.item_focus = None;
                        map.x = ox + x - px;
                        map.y = oy + y - py;
                        map.clamp();
                        unsafe {
                            qt::sl_cursor(map.widget, 1);
                        }
                        repaint = true;
                    }
                } else if map.moving() || map.incoming.is_some() {
                    unsafe {
                        qt::sl_tooltip(map.widget, x, y, qt::c("").as_ptr());
                    }
                } else if let Some((entry, _, _)) = map.hit(x, y) {
                    let exact = entry.size.map_or("Unknown size".into(), |n| {
                        format!("{} bytes", crate::scanner::commas(n))
                    });
                    unsafe {
                        qt::sl_tooltip(
                            map.widget,
                            x,
                            y,
                            qt::c(&format!("{}\n{}", entry.name, exact)).as_ptr(),
                        );
                    }
                } else {
                    unsafe {
                        qt::sl_tooltip(map.widget, x, y, qt::c("").as_ptr());
                    }
                }
            }
            3 => {
                map.press = None;
                map.pending_click = false;
                unsafe {
                    qt::sl_tooltip(map.widget, x, y, qt::c("").as_ptr());
                }
                let point = (x, y);
                let mut anchor = point;
                if delta > 0. {
                    let retained = map.wheel_focus.filter(|(_, p, time)| {
                        time.elapsed().as_millis() < 350
                            && (p.0 - x).hypot(p.1 - y) < 36. * SCALE.get()
                    });
                    let rect = retained.map(|(r, _, _)| r).or_else(|| map.nearby(point));
                    if let Some(r) = rect {
                        anchor = map.item_anchor(r);
                        map.wheel_focus = Some((r, point, Instant::now()));
                        map.item_focus = Some(r);
                    }
                } else {
                    map.wheel_focus = None;
                }
                map.zoom_to(
                    map.target * 1.18f64.powf((delta / 120.).clamp(-8., 8.)),
                    anchor,
                    true,
                );
                repaint = true;
            }
            7 if button == 1 => {
                map.press = None;
                map.pending_click = false;
                map.focus_at((x, y));
                repaint = true;
            }
            8 if button == 1 => {
                map.pending_click = map.press.take().is_some() && !map.dragging;
                map.dragging = false;
                unsafe {
                    qt::sl_cursor(map.widget, 0);
                }
            }
            4 => {
                map.width = x;
                map.height = y;
                map.layout.clear();
                map.reset();
                repaint = true;
            }
            5 => {
                let value = if button == 48 {
                    1.
                } else {
                    map.target * if button == 45 { 1. / 1.2 } else { 1.2 }
                };
                if button == 48 {
                    map.item_focus = None;
                    map.wheel_focus = None;
                }
                let anchor = map
                    .item_focus
                    .map(|r| map.item_anchor(r))
                    .unwrap_or((map.width / 2., map.height / 2.));
                map.zoom_to(value, anchor, true);
                repaint = true;
            }
            6 => unsafe {
                qt::sl_tooltip(map.widget, 0., 0., qt::c("").as_ptr());
            },
            _ => {}
        }
        if repaint && !map.widget.is_null() {
            unsafe {
                qt::sl_update(map.widget);
            }
        }
        None
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_paint(id: i32, painter: *mut c_void, width: i32, height: i32, ratio: f64) {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        let map = &mut maps[id as usize];
        let dark = DARK.get();
        let scale = SCALE.get();
        let p = painter;
        let canvas = Rect {
            x: 0.,
            y: 0.,
            w: width as f64,
            h: height as f64,
        };
        if id == 3 {
            qt::fill(p, canvas, if dark { "#222830" } else { "#f7f9fb" }, 0.);
            let color = if dark { "#80c8d4" } else { "#1b6c80" };
            let factor = canvas.w.min(canvas.h) / 112.;
            for (i, r) in [
                Rect {
                    x: 12.,
                    y: 12.,
                    w: 42.,
                    h: 88.,
                },
                Rect {
                    x: 60.,
                    y: 12.,
                    w: 40.,
                    h: 40.,
                },
                Rect {
                    x: 60.,
                    y: 58.,
                    w: 40.,
                    h: 42.,
                },
            ]
            .into_iter()
            .enumerate()
            {
                let alpha = if REDUCED.get() {
                    0.4
                } else {
                    0.34 + 0.12
                        * (2. * std::f64::consts::PI * (map.phase / 100. + i as f64 / 3.)).sin()
                };
                let argb = format!("#{:02x}{}", (alpha * 255.).round() as u8, &color[1..]);
                qt::fill(
                    p,
                    Rect {
                        x: r.x * factor,
                        y: r.y * factor,
                        w: r.w * factor,
                        h: r.h * factor,
                    },
                    &argb,
                    7. * factor,
                );
            }
            unsafe {
                qt::sl_outline(
                    p,
                    4. * factor,
                    4. * factor,
                    104. * factor,
                    104. * factor,
                    qt::c(color).as_ptr(),
                    14. * factor,
                    2. * factor,
                );
            }
            return;
        }
        if id == 2 {
            qt::fill(p, canvas, if dark { "#1b232b" } else { "#e8eef2" }, 0.);
            let maximum = map.entries.iter().filter_map(|e| e.size).max().unwrap_or(0);
            let values = map
                .entries
                .iter()
                .map(|e| {
                    size_text(e.size)
                        + if e.partial && e.size.is_some() {
                            " +"
                        } else {
                            ""
                        }
                })
                .collect::<Vec<_>>();
            let inside = 12. * scale;
            let row_width = (canvas.w - 2. * inside).max(0.);
            let value_width = values
                .iter()
                .map(|v| unsafe { qt::sl_text_width(p, qt::c(v).as_ptr()) })
                .fold(0., f64::max)
                .add(2. * scale)
                .min(row_width * 0.55);
            let name_width = (row_width - value_width - 12. * scale).max(0.);
            for (i, entry) in map.entries.iter().enumerate() {
                let y = i as f64 * 64. * scale;
                qt::fill(
                    p,
                    Rect {
                        x: 0.,
                        y,
                        w: canvas.w,
                        h: 56. * scale,
                    },
                    if dark { "#2a3540" } else { "#fafcfd" },
                    8. * scale,
                );
                qt::draw_text(
                    p,
                    Rect {
                        x: inside,
                        y: y + 7. * scale,
                        w: name_width,
                        h: 24. * scale,
                    },
                    &entry.name,
                    if dark { "#edf2f7" } else { "#202d37" },
                    false,
                    1,
                );
                qt::draw_text(
                    p,
                    Rect {
                        x: inside + row_width - value_width,
                        y: y + 7. * scale,
                        w: value_width,
                        h: 24. * scale,
                    },
                    &values[i],
                    if dark { "#acb8c6" } else { "#586c7b" },
                    true,
                    0,
                );
                if let Some(size) = entry.size.filter(|_| maximum > 0) {
                    qt::fill(
                        p,
                        Rect {
                            x: inside,
                            y: y + 39. * scale,
                            w: row_width * size as f64 / maximum as f64,
                            h: 4. * scale,
                        },
                        if dark { "#8ccbd5" } else { "#36798b" },
                        2. * scale,
                    );
                }
            }
            if map.entries.is_empty() {
                qt::draw_text(
                    p,
                    Rect { x: 12., ..canvas },
                    "No items in this view.",
                    if dark { "#acb8c6" } else { "#5d5f60" },
                    false,
                    0,
                );
            }
            return;
        }
        qt::fill(p, canvas, if dark { "#171e25" } else { "#e7eef2" }, 0.);
        if map.width != canvas.w || map.height != canvas.h {
            map.width = canvas.w;
            map.height = canvas.h;
            map.layout.clear();
        }
        if map.layout.is_empty() {
            map.layout.build(
                map.entries.iter().map(|e| e.size.unwrap_or(0)),
                // Borders belong to screen-space painting; scene-space padding grows with zoom.
                Rect {
                    x: 0.,
                    y: 0.,
                    w: canvas.w,
                    h: canvas.h,
                },
            );
            map.zoom_limit = map.layout.zoom_limit(canvas.w, canvas.h);
        }
        if map.layout.is_empty() {
            qt::draw_text(
                p,
                Rect {
                    x: 16.,
                    w: canvas.w - 32.,
                    ..canvas
                },
                &map.message,
                if dark { "#acb8c6" } else { "#5d5f60" },
                false,
                0,
            );
            return;
        }
        let transform = map.transform();
        map.layout.visible(
            transform.scene(Rect {
                x: -1.,
                y: -1.,
                w: canvas.w + 2.,
                h: canvas.h + 2.,
            }),
            &mut map.visible,
        );
        let line = unsafe { qt::sl_line_height(p) };
        let font_key = unsafe { qt::sl_font_key(p) };
        if font_key != map.font_key {
            map.labels.clear();
            map.font_key = font_key;
        }
        for v in 0..map.visible.len() {
            let node = map.layout.nodes[map.visible[v]];
            let i = node.item;
            let transformed = transform.rect(node.rect);
            let gutter = 1. / ratio;
            let snap = |x: f64| (x * ratio).round_ties_even() / ratio;
            let box_ = Rect {
                x: snap(transformed.x) + gutter,
                y: snap(transformed.y) + gutter,
                w: snap(transformed.x + transformed.w) - snap(transformed.x) - 2. * gutter,
                h: snap(transformed.y + transformed.h) - snap(transformed.y) - 2. * gutter,
            };
            let visible = box_.intersect(canvas);
            if visible.w <= 0. || visible.h <= 0. {
                continue;
            }
            let entry = &map.entries[i];
            unsafe {
                qt::sl_map_fill(
                    p,
                    visible.x,
                    visible.y,
                    visible.w,
                    visible.h,
                    if entry.path.as_os_str().is_empty() {
                        4
                    } else {
                        (i % 4) as i32
                    },
                    dark as i32,
                );
            }
            // Neither label orientation can fit when its short side is below one text line plus padding.
            if visible.w.min(visible.h) < line + 16. * scale {
                continue;
            }
            let run = map.labels.entry(i).or_insert_with(|| {
                let value = size_text(entry.size) + if entry.partial { " +" } else { "" };
                TextRun(unsafe {
                    qt::sl_text_run_new(
                        entry.name.as_ptr(),
                        entry.name.len(),
                        value.as_ptr(),
                        value.len(),
                    )
                })
            });
            let name_width = unsafe { qt::sl_text_run_width(p, run.0) };
            let vertical = vertical_label(visible, name_width, scale, map.angles[i]);
            let inner = Rect {
                x: visible.x + 8. * scale,
                y: visible.y + 8. * scale,
                w: visible.w - 16. * scale,
                h: visible.h - 16. * scale,
            };
            let (w, h) = if vertical {
                (inner.h, inner.w)
            } else {
                (inner.w, inner.h)
            };
            if w < name_width.min(32. * scale) || h < line {
                continue;
            }
            map.angles[i] = vertical;
            unsafe {
                qt::sl_save(p);
                qt::sl_clip(p, visible.x, visible.y, visible.w, visible.h);
                if vertical {
                    qt::sl_translate(p, inner.x, inner.y + inner.h, -90.);
                } else {
                    qt::sl_translate(p, inner.x, inner.y, 0.);
                }
            }
            unsafe {
                qt::sl_text_run_draw(p, run.0, 0., 0., w, line, 0, dark as i32);
            }
            if h >= 2. * line + 3. * scale {
                unsafe {
                    qt::sl_text_run_draw(p, run.0, 0., line + 3. * scale, w, line, 1, dark as i32);
                }
            }
            unsafe {
                qt::sl_restore(p);
            }
        }
    });
}
use std::ops::Add;

#[cfg(test)]
mod viewport_tests {
    use super::*;
    #[test]
    fn zoom_and_pan_keep_map_bounds_covering_viewport() {
        let mut map = Canvas::default();
        map.width = 340.;
        map.height = 240.;
        // This is the same full-viewport scene rectangle used by the painter.
        map.layout.build(
            [90_u64, 5, 5].into_iter(),
            Rect {
                x: 0.,
                y: 0.,
                w: map.width,
                h: map.height,
            },
        );
        for zoom in [1., 19.56, 195.56, 1e6] {
            map.zoom = zoom;
            for (x, y) in [(0., 0.), (-1e15, -1e15), (1e15, 1e15)] {
                map.x = x;
                map.y = y;
                map.clamp();
                let root = map.transform().rect(map.layout.nodes[0].rect);
                assert!(root.x <= 0. && root.y <= 0.);
                assert!(root.x + root.w >= map.width - 1e-6);
                assert!(root.y + root.h >= map.height - 1e-6);
                let mut visible = Vec::new();
                map.layout.visible(
                    map.transform().scene(Rect {
                        x: 0.,
                        y: 0.,
                        w: map.width,
                        h: map.height,
                    }),
                    &mut visible,
                );
                let covered: f64 = visible
                    .iter()
                    .map(|i| {
                        map.transform()
                            .rect(map.layout.nodes[*i].rect)
                            .intersect(Rect {
                                x: 0.,
                                y: 0.,
                                w: map.width,
                                h: map.height,
                            })
                    })
                    .map(|r| r.w * r.h)
                    .sum();
                assert!((covered - map.width * map.height).abs() < 0.1);
            }
        }
    }
}
