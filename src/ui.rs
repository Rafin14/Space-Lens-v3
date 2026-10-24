use crate::{
    canvas,
    geometry::Rect,
    models::{self, Mode},
    native,
    qt::{self, NULL, W},
    scanner::{Snapshot, commas, size_text},
    session::{Session, State},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{CStr, c_char},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Instant,
};

const CHOOSE: i32 = 1;
const REFRESH: i32 = 2;
const BACK: i32 = 3;
const FORWARD: i32 = 4;
const UP: i32 = 5;
const FOLDERS: i32 = 6;
const FILES: i32 = 7;
const LIGHT: i32 = 8;
const DARK: i32 = 9;
const SCALE: i32 = 10;
const HELP: i32 = 11;
const SLIDER: i32 = 12;
const PERCENT: i32 = 13;
const REDUCED: i32 = 14;
const COLLAPSE: i32 = 15;
const CANCEL: i32 = 16;
const ISSUES: i32 = 17;
const ALL: i32 = 84;
const TREE: i32 = 30;
const MAIN_SELECT: i32 = 40;
const MAIN_DELETE: i32 = 42;
const MAIN_COPY: i32 = 43;
const EXPAND: i32 = 50;
const MAP_CLOSE: i32 = 60;
const MAP_ALL: i32 = 61;
const MAP_FOLDERS: i32 = 62;
const MAP_FILES: i32 = 63;
const MAP_SELECT: i32 = 70;
const MAP_DELETE: i32 = 72;
const MAP_COPY: i32 = 73;
const MAP_OUT: i32 = 80;
const MAP_IN: i32 = 81;
const MAP_FIT: i32 = 82;
const MAIN_FIT: i32 = 83;
const ISSUES_CLOSE: i32 = 91;

#[derive(Clone, Debug)]
pub struct Event {
    pub id: i32,
    pub a: i64,
    pub b: i64,
    pub x: f64,
    pub y: f64,
    pub delta: f64,
    pub text: String,
}
thread_local! {
    static EVENTS:RefCell<Vec<Event>>=const{RefCell::new(vec![])};
    pub static APP:RefCell<Option<App>>=const{RefCell::new(None)};
}
static BUSY: AtomicBool = AtomicBool::new(false);
fn handle_events(app: &mut App) -> bool {
    let events = EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
    let mut render = false;
    for event in events {
        render |= app.handle(event);
    }
    render
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_input() {
    APP.with(|slot| {
        let Ok(mut app) = slot.try_borrow_mut() else {
            return;
        };
        if let Some(app) = app.as_mut() {
            if handle_events(app) {
                app.render();
            }
        }
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_event(
    id: i32,
    a: i64,
    b: i64,
    x: f64,
    y: f64,
    delta: f64,
    text: *const c_char,
) {
    let value = if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text).to_string_lossy().into_owned() }
    };
    EVENTS.with(|events| {
        events.borrow_mut().push(Event {
            id,
            a,
            b,
            x,
            y,
            delta,
            text: value,
        })
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_can_close(_kind: i32) -> i32 {
    (!BUSY.load(Ordering::Relaxed)) as i32
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_tick() {
    APP.with(|app| {
        let Ok(mut slot) = app.try_borrow_mut() else {
            return;
        };
        if let Some(app) = slot.as_mut() {
            let mut render = handle_events(app);
            render |= app.session.poll();
            if let Some(receiver) = &app.deleting {
                if let Ok((result, target, changed)) = receiver.try_recv() {
                    app.deleting = None;
                    BUSY.store(false, Ordering::Relaxed);
                    app.close_map();
                    if changed {
                        app.session.refresh_after_delete(&target);
                    }
                    app.session.message = match result {
                        Ok(()) => "Deletion finished. Refreshing measurements…".into(),
                        Err(native::DeleteError::Cancelled) => if changed {
                            "Deletion cancelled or skipped in Windows. Refreshing the affected folder…".into()
                        } else {
                            "Deletion cancelled in Windows. The file is unchanged.".into()
                        },
                        Err(native::DeleteError::Failure(error)) => error,
                    };
                    render = true;
                }
            }
            if canvas::animate() {
                app.zoom_labels();
            }
            if app.session.state == State::Loading && !app.preferences.reduced {
                canvas::loading_tick();
            }
            if app.session.state == State::Loading && app.last_loading.elapsed().as_millis() >= 200
            {
                render = true;
                app.last_loading = Instant::now();
            }
            if render {
                app.render();
            }
            let interval = if canvas::needs_animation()
                || (app.session.state == State::Loading && !app.preferences.reduced)
            {
                16
            } else if app.session.state == State::Loading {
                1000
            } else {
                0
            };
            unsafe {
                qt::sl_timer_mode(interval);
            }
        } else {
            unsafe {
                qt::sl_timer_mode(0);
            }
        }
    });
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub dark: bool,
    pub scale: u32,
    pub reduced: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: false,
            scale: 100,
            reduced: native::reduced_motion(),
        }
    }
}
fn preference_path() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default())
        .join("SpaceLens_Rust/settings.json")
}

pub struct MainUi {
    pub central: W,
    pub sidebar: W,
    pub split: W,
    pub tree: W,
    pub path: W,
    pub total: W,
    pub summary: W,
    pub stamp: W,
    pub notice: W,
    pub choose: W,
    pub refresh: W,
    pub nav: [W; 3],
    pub modes: [W; 3],
    pub themes: [W; 2],
    pub scale: W,
    pub collapse: W,
    pub issues: W,
    pub retry: W,
    pub workspace: W,
    pub content: W,
    pub tabs: W,
    pub overview: W,
    pub overview_scroll: W,
    pub table_panel: W,
    pub table: W,
    pub table_empty: W,
    pub table_stack: W,
    pub map: W,
    pub ranks: W,
    pub rank_scroll: W,
    pub map_caption: W,
    pub map_zoom: W,
    pub expand: W,
    pub count: W,
    pub selected: W,
    pub delete: W,
    pub delete_action: W,
    pub loading: W,
    pub loading_title: W,
    pub loading_body: W,
    pub loading_glyph: W,
    pub loading_stats: W,
    pub loading_counts: [W; 3],
    pub loading_elapsed: W,
    pub loading_cancel: W,
}
pub struct MapUi {
    pub dialog: W,
    pub path: W,
    pub total: W,
    pub nav: [W; 3],
    pub modes: [W; 3],
    pub map: W,
    pub table: W,
    pub notice: W,
    pub count: W,
    pub selected: W,
    pub delete: W,
    pub delete_action: W,
    pub zoom: W,
}
pub struct ScaleUi {
    pub dialog: W,
    slider: W,
    percent: W,
    reduced: W,
}
pub struct App {
    pub window: W,
    pub ui: MainUi,
    pub map_ui: Option<MapUi>,
    pub session: Session,
    pub preferences: Preferences,
    pub mode: Mode,
    pub map_mode: Mode,
    pub scale_ui: Option<ScaleUi>,
    scale_preview: Option<u32>,
    issues_ui: Option<W>,
    pub expanded: bool,
    compact: bool,
    persist: bool,
    rendered: Option<(u64, PathBuf, Mode, State)>,
    map_rendered: Option<(u64, PathBuf, Mode)>,
    expanded_paths: HashSet<PathBuf>,
    shown_location: PathBuf,
    deleting: Option<Receiver<(Result<(), native::DeleteError>, PathBuf, bool)>>,
    last_loading: Instant,
    scan_started: Instant,
    pending_origin: Option<Rect>,
    pub started: Instant,
}

fn s(value: i32, scale: f64) -> i32 {
    (value as f64 * scale).round_ties_even() as i32
}
fn layout(parent: W, horizontal: bool, margin: i32, gap: i32, scale: f64) -> W {
    qt::layout(parent, horizontal, s(margin, scale), s(gap, scale))
}
fn label(parent: W, text: &str, name: &str, wrap: bool) -> W {
    qt::label(parent, text, name, wrap)
}
fn nav(parent: W, scale: f64) -> [W; 3] {
    std::array::from_fn(|i| {
        let (name, shortcut, event) = [
            ("Back", "Alt+Left", BACK),
            ("Forward", "Alt+Right", FORWARD),
            ("Up", "Alt+Up", UP),
        ][i];
        let button = qt::button(parent, "", "navigation", event);
        qt::set(button, "accessibleName", name);
        qt::set(button, "toolTip", &format!("{name} · {shortcut}"));
        unsafe {
            qt::sl_navigation_icon(button, i as i32, scale);
        }
        button
    })
}
fn segments(parent: W, choices: &[(&str, i32)], scale: f64) -> (W, Vec<W>) {
    let control = qt::widget(0, parent, "segmentedControl", 0);
    let row = layout(control, true, 3, 2, scale);
    let buttons = choices
        .iter()
        .map(|(name, id)| {
            let button = qt::button(control, name, "segment", *id);
            qt::boolean(button, "checkable", true);
            qt::set(
                button,
                "accessibleName",
                &format!("Show {}", name.to_lowercase()),
            );
            qt::add(row, button, 0);
            button
        })
        .collect::<Vec<_>>();
    unsafe {
        qt::sl_group(control, buttons.as_ptr(), buttons.len() as i32);
    }
    (control, buttons)
}
fn delete_controls(
    parent: W,
    table: W,
    selected: W,
    event: i32,
    copy: i32,
    scale: f64,
) -> (W, W, W) {
    let row = layout(NULL, true, 0, 8, scale);
    unsafe {
        qt::sl_ignored(selected);
    }
    qt::add(row, selected, 1);
    unsafe {
        qt::sl_action(
            table,
            qt::c("Copy path").as_ptr(),
            qt::c("Ctrl+C").as_ptr(),
            copy,
            0,
        );
    }
    unsafe {
        qt::sl_action(table, qt::c("").as_ptr(), qt::c("").as_ptr(), 0, 1);
    }
    let action = unsafe {
        qt::sl_action(
            table,
            qt::c("Delete…").as_ptr(),
            qt::c("Delete").as_ptr(),
            event,
            0,
        )
    };
    let button = qt::button(parent, "Delete", "", event);
    qt::set(
        button,
        "accessibleName",
        "Delete selected item using Windows",
    );
    qt::set(
        button,
        "toolTip",
        "Delete the selected item using Windows. Windows handles Recycle Bin and permanent-deletion prompts.",
    );
    qt::add(row, button, 0);
    (row, button, action)
}

impl MainUi {
    fn build(window: W, scale: f64) -> Self {
        let central = qt::widget(0, NULL, "chrome", 0);
        let outer = layout(central, false, 0, 0, scale);
        qt::margins(outer, s(16, scale), 0, s(8, scale), s(8, scale));
        let top = layout(NULL, true, 0, 10, scale);
        qt::margins(top, s(8, scale), s(8, scale), s(16, scale), s(8, scale));
        let mark = label(central, "", "", false);
        unsafe {
            qt::sl_brand(mark, s(26, scale));
        }
        qt::add(top, mark, 0);
        qt::add(top, label(central, "Space Lens", "brand", false), 0);
        qt::add(top, label(central, "v3", "muted", false), 0);
        qt::stretch(top);
        let light = qt::button(central, "Light", "theme", LIGHT);
        let dark = qt::button(central, "Dark", "theme", DARK);
        qt::boolean(light, "checkable", true);
        qt::boolean(dark, "checkable", true);
        unsafe {
            qt::sl_group(central, [light, dark].as_ptr(), 2);
        }
        qt::add(top, light, 0);
        qt::add(top, dark, 0);
        let scale_button = qt::button(central, "Scale 100%", "", SCALE);
        qt::set(
            scale_button,
            "accessibleName",
            "Interface scale and motion settings",
        );
        qt::add(top, scale_button, 0);
        qt::add(top, qt::button(central, "Shortcuts", "", HELP), 0);
        let choose = qt::button(central, "Choose folder", "primary", CHOOSE);
        qt::set(choose, "toolTip", "Choose a folder · Ctrl+O");
        qt::add(top, choose, 0);
        qt::sub(outer, top, 0);
        let split = qt::widget(9, central, "", 0);
        qt::int(split, "handleWidth", s(8, scale));
        let sidebar = qt::widget(0, NULL, "sidebar", 0);
        qt::int(sidebar, "minimumWidth", s(220, scale));
        let sidebar_layout = layout(sidebar, false, 0, 12, scale);
        qt::margins(
            sidebar_layout,
            s(8, scale),
            s(20, scale),
            s(12, scale),
            s(12, scale),
        );
        let title = layout(NULL, true, 0, 8, scale);
        qt::add(
            title,
            label(sidebar, "Folder hierarchy", "section", false),
            1,
        );
        let collapse = qt::button(sidebar, "Collapse all", "quiet", COLLAPSE);
        qt::set(
            collapse,
            "toolTip",
            "Collapse every folder without changing the current location",
        );
        qt::add(title, collapse, 0);
        qt::sub(sidebar_layout, title, 0);
        qt::add(
            sidebar_layout,
            label(
                sidebar,
                "The selected scan stays in this folder.",
                "muted",
                true,
            ),
            0,
        );
        let tree = qt::widget(6, sidebar, "", 0);
        qt::set(tree, "accessibleName", "Folder hierarchy");
        unsafe {
            qt::sl_view(tree, 0, TREE, scale);
        }
        qt::add(sidebar_layout, tree, 1);
        qt::add(
            sidebar_layout,
            label(
                sidebar,
                "Snapshot scan\nLinks and reparse points are excluded.",
                "footer",
                true,
            ),
            0,
        );
        unsafe {
            qt::sl_split_add(split, sidebar);
        }
        let sheet = qt::widget(0, NULL, "sheet", 0);
        let body = layout(sheet, false, 0, 16, scale);
        qt::margins(body, s(24, scale), s(20, scale), s(24, scale), s(16, scale));
        let heading = layout(NULL, true, 0, 8, scale);
        let path = qt::widget(16, sheet, "location", 0);
        qt::text(path, "No folder selected");
        qt::add(heading, path, 1);
        let actions = layout(NULL, false, 0, 8, scale);
        let refresh = qt::button(sheet, "Refresh scan", "", REFRESH);
        qt::add(actions, refresh, 0);
        let stamp = label(sheet, "Local · Windows deletion", "stamp", false);
        unsafe {
            qt::sl_align(stamp, 2 | 128);
        }
        qt::add(actions, stamp, 0);
        qt::sub(heading, actions, 0);
        qt::sub(body, heading, 0);
        let summary_row = layout(NULL, true, 0, 12, scale);
        let total = label(sheet, "—", "total", false);
        qt::add(summary_row, total, 0);
        let summary = label(sheet, "Choose a folder to begin.", "muted", true);
        qt::add(summary_row, summary, 1);
        let (mode_control, modes) = segments(
            sheet,
            &[("All items", ALL), ("Folders", FOLDERS), ("Files", FILES)],
            scale,
        );
        qt::add(summary_row, mode_control, 0);
        qt::sub(body, summary_row, 0);
        let notices = layout(NULL, true, 0, 8, scale);
        let notice = label(sheet, "", "notice", true);
        unsafe {
            qt::sl_ignored(notice);
        }
        qt::add(notices, notice, 1);
        let retry = qt::button(sheet, "Retry scan", "", REFRESH);
        let issues = qt::button(sheet, "Scan details", "", ISSUES);
        qt::add(notices, retry, 0);
        qt::add(notices, issues, 0);
        let navigation = nav(sheet, scale);
        for button in navigation {
            qt::add(notices, button, 0);
        }
        qt::sub(body, notices, 0);
        let workspace = qt::widget(10, sheet, "", 0);
        let content = qt::widget(9, NULL, "", 0);
        qt::int(content, "handleWidth", s(16, scale));
        let overview = qt::widget(0, NULL, "", 0);
        let overview_layout = layout(overview, false, 0, 10, scale);
        let map_header = layout(NULL, true, 0, 8, scale);
        qt::add(
            map_header,
            label(overview, "Space map", "section", false),
            1,
        );
        let expand = qt::button(overview, "Expand space map", "", EXPAND);
        qt::add(map_header, expand, 0);
        qt::sub(overview_layout, map_header, 0);
        let map_caption = label(overview, "Largest 20 + Other · Logical size", "muted", true);
        qt::add(overview_layout, map_caption, 0);
        let map = qt::widget(5, overview, "", 0);
        qt::set(map, "accessibleName", "Space map");
        qt::int(map, "minimumHeight", s(150, scale));
        qt::add(overview_layout, map, 1);
        canvas::bind(0, map);
        let map_tools = layout(NULL, true, 0, 8, scale);
        qt::add(
            map_tools,
            label(overview, "Scroll to zoom · Drag to pan", "footer", false),
            1,
        );
        let map_zoom = label(overview, "100%", "muted", false);
        qt::add(map_tools, map_zoom, 0);
        qt::add(map_tools, qt::button(overview, "Fit", "", MAIN_FIT), 0);
        qt::sub(overview_layout, map_tools, 0);
        let ranking_panel = qt::widget(0, overview, "rankingPanel", 0);
        let ranking_layout = layout(ranking_panel, false, 14, 10, scale);
        qt::add(
            ranking_layout,
            label(ranking_panel, "Largest items", "rankingTitle", false),
            0,
        );
        let rank_scroll = qt::widget(8, ranking_panel, "rankingArea", 0);
        qt::int(rank_scroll, "minimumHeight", s(184, scale));
        let ranks = qt::widget(5, NULL, "", 2);
        canvas::bind(2, ranks);
        unsafe {
            qt::sl_scroll_child(rank_scroll, ranks);
        }
        qt::add(ranking_layout, rank_scroll, 1);
        qt::add(overview_layout, ranking_panel, 1);
        let overview_scroll = qt::widget(8, NULL, "", 0);
        qt::set(
            overview_scroll,
            "accessibleName",
            "Scrollable space overview",
        );
        unsafe {
            qt::sl_scroll_child(overview_scroll, overview);
            qt::sl_split_add(content, overview_scroll);
        }
        let table_panel = qt::widget(0, NULL, "breakdownPanel", 0);
        let table_layout = layout(table_panel, false, 16, 12, scale);
        let table_heading = layout(NULL, true, 0, 8, scale);
        qt::add(
            table_heading,
            label(table_panel, "Breakdown", "section", false),
            1,
        );
        let count = label(table_panel, "All items", "muted", false);
        qt::add(table_heading, count, 0);
        qt::sub(table_layout, table_heading, 0);
        let table = qt::widget(7, table_panel, "", 0);
        qt::set(
            table,
            "accessibleName",
            "Complete file and folder breakdown",
        );
        unsafe {
            qt::sl_view(table, 1, MAIN_SELECT, scale);
        }
        let table_empty = label(
            table_panel,
            "Choose a folder to see its contents.",
            "muted",
            true,
        );
        unsafe {
            qt::sl_align(table_empty, 4 | 128);
        }
        let table_stack = qt::widget(10, table_panel, "", 0);
        unsafe {
            qt::sl_stack_add(table_stack, table);
            qt::sl_stack_add(table_stack, table_empty);
        }
        qt::add(table_layout, table_stack, 1);
        let selected = label(
            table_panel,
            "Enter opens a folder · Ctrl+C copies its path",
            "footer",
            true,
        );
        let (row, delete, delete_action) =
            delete_controls(table_panel, table, selected, MAIN_DELETE, MAIN_COPY, scale);
        qt::sub(table_layout, row, 0);
        unsafe {
            qt::sl_split_add(content, table_panel);
            qt::sl_split_sizes(content, 340, 620);
            qt::sl_split_stretch(content, 1, 2);
        }
        let tabs = qt::widget(11, NULL, "", 0);
        qt::set(tabs, "accessibleName", "Workspace views");
        let loading = qt::widget(0, NULL, "", 0);
        let loading_layout = layout(loading, false, 16, 16, scale);
        qt::stretch(loading_layout);
        let loading_glyph = qt::widget(5, loading, "", 3);
        canvas::bind(3, loading_glyph);
        unsafe {
            qt::sl_fixed(loading_glyph, s(112, scale), s(112, scale));
            qt::sl_add_center(loading_layout, loading_glyph);
        }
        let loading_title = label(loading, "See where your space goes", "title", true);
        let loading_body = label(
            loading,
            "Choose a folder to explore its files, folders, and size breakdown.",
            "muted",
            true,
        );
        for widget in [loading_title, loading_body] {
            unsafe {
                qt::sl_align(widget, 4 | 128);
            }
            qt::add(loading_layout, widget, 0);
        }
        let loading_stats = qt::widget(0, loading, "", 0);
        let stats = layout(loading_stats, true, 0, 8, scale);
        let loading_counts = std::array::from_fn(|i| {
            let block = layout(NULL, false, 0, 6, scale);
            let value = label(loading_stats, "0", "section", false);
            let caption = label(
                loading_stats,
                ["Files measured", "Folders read", "Logical bytes found"][i],
                "muted",
                false,
            );
            unsafe {
                qt::sl_align(value, 4 | 128);
                qt::sl_align(caption, 4 | 128);
            }
            qt::add(block, value, 0);
            qt::add(block, caption, 0);
            qt::sub(stats, block, 1);
            value
        });
        qt::add(loading_layout, loading_stats, 0);
        let loading_elapsed = label(loading, "Starting…", "muted", true);
        unsafe {
            qt::sl_align(loading_elapsed, 4 | 128);
        }
        qt::add(loading_layout, loading_elapsed, 0);
        let loading_cancel = qt::button(loading, "Choose folder", "", 18);
        unsafe {
            qt::sl_add_center(loading_layout, loading_cancel);
        }
        qt::stretch(loading_layout);
        unsafe {
            qt::sl_stack_add(workspace, content);
            qt::sl_stack_add(workspace, tabs);
            qt::sl_stack_add(workspace, loading);
        }
        qt::add(body, workspace, 1);
        qt::add(
            body,
            label(
                sheet,
                "Logical file size, not allocated space. Snapshots update when you refresh.",
                "footer",
                true,
            ),
            0,
        );
        unsafe {
            qt::sl_split_add(split, sheet);
            qt::sl_split_sizes(split, s(270, scale), s(1060, scale));
            qt::sl_split_stretch(split, 1, 1);
            qt::sl_central(window, central);
        }
        qt::add(outer, split, 1);
        Self {
            central,
            sidebar,
            split,
            tree,
            path,
            total,
            summary,
            stamp,
            notice,
            choose,
            refresh,
            nav: navigation,
            modes: [modes[0], modes[1], modes[2]],
            themes: [light, dark],
            scale: scale_button,
            collapse,
            issues,
            retry,
            workspace,
            content,
            tabs,
            overview,
            overview_scroll,
            table_panel,
            table,
            table_empty,
            table_stack,
            map,
            ranks,
            rank_scroll,
            map_caption,
            map_zoom,
            expand,
            count,
            selected,
            delete,
            delete_action,
            loading,
            loading_title,
            loading_body,
            loading_glyph,
            loading_stats,
            loading_counts,
            loading_elapsed,
            loading_cancel,
        }
    }
}

impl MapUi {
    fn build(window: W, scale: f64) -> Self {
        let dialog = qt::widget(4, window, "mapWorkspace", MAP_CLOSE);
        unsafe {
            qt::sl_dialog(dialog, 1);
        }
        qt::set(dialog, "windowTitle", "Space map · Space Lens v3");
        let layout_ = layout(dialog, false, 0, 16, scale);
        qt::margins(
            layout_,
            s(28, scale),
            s(24, scale),
            s(28, scale),
            s(24, scale),
        );
        let header = layout(NULL, true, 0, 8, scale);
        let path = qt::widget(16, dialog, "location", 0);
        qt::add(header, path, 1);
        qt::add(
            header,
            qt::button(dialog, "Exit full screen", "", MAP_CLOSE),
            0,
        );
        qt::sub(layout_, header, 0);
        let total = label(dialog, "", "section", false);
        let tools = layout(NULL, true, 0, 8, scale);
        let (control, buttons) = segments(
            dialog,
            &[
                ("All items", MAP_ALL),
                ("Folders", MAP_FOLDERS),
                ("Files", MAP_FILES),
            ],
            scale,
        );
        qt::add(tools, control, 0);
        qt::stretch(tools);
        qt::add(tools, total, 0);
        qt::add(tools, label(dialog, "", "", false), 0);
        let zoom = label(dialog, "100%", "muted", false);
        qt::add(tools, qt::button(dialog, "−", "", MAP_OUT), 0);
        qt::add(tools, zoom, 0);
        qt::add(tools, qt::button(dialog, "+", "", MAP_IN), 0);
        qt::add(tools, qt::button(dialog, "Fit map", "", MAP_FIT), 0);
        qt::sub(layout_, tools, 0);
        let split = qt::widget(9, dialog, "", 0);
        qt::int(split, "handleWidth", s(20, scale));
        let map_panel = qt::widget(0, NULL, "", 0);
        let map_layout = layout(map_panel, false, 0, 12, scale);
        let map_header = layout(NULL, true, 0, 12, scale);
        let notice = label(map_panel, "", "muted", true);
        qt::add(map_header, notice, 1);
        let navigation = nav(map_panel, scale);
        for button in navigation {
            qt::add(map_header, button, 0);
        }
        qt::sub(map_layout, map_header, 0);
        let map = qt::widget(5, map_panel, "", 1);
        qt::add(map_layout, map, 1);
        qt::set(map, "accessibleName", "Expanded space map");
        qt::int(map, "minimumWidth", s(300, scale));
        canvas::bind(1, map);
        unsafe {
            qt::sl_split_add(split, map_panel);
        }
        let details = qt::widget(0, NULL, "mapDetails", 0);
        qt::int(details, "minimumWidth", s(380, scale));
        let details_layout = layout(details, false, 16, 16, scale);
        let count = label(details, "Breakdown", "section", false);
        qt::add(details_layout, count, 0);
        let table = qt::widget(7, details, "", 0);
        qt::set(table, "accessibleName", "Expanded map complete breakdown");
        unsafe {
            qt::sl_view(table, 2, MAP_SELECT, scale);
        }
        qt::add(details_layout, table, 1);
        let selected = label(
            details,
            "Select a file for its exact size. Enter opens a folder.",
            "footer",
            true,
        );
        let (row, delete, delete_action) =
            delete_controls(details, table, selected, MAP_DELETE, MAP_COPY, scale);
        qt::sub(details_layout, row, 0);
        unsafe {
            qt::sl_split_add(split, details);
            qt::sl_split_sizes(split, s(1100, scale), s(520, scale));
        }
        qt::add(layout_, split, 1);
        qt::add(
            layout_,
            label(
                dialog,
                "Scroll to zoom at the pointer · Drag to pan · Click a folder to explore · Escape exits",
                "footer",
                true,
            ),
            0,
        );
        for (key, id) in [
            ("Alt+Left", BACK),
            ("Alt+Right", FORWARD),
            ("Alt+Up", UP),
            ("0", MAP_FIT),
        ] {
            qt::shortcut(dialog, key, id);
        }
        Self {
            dialog,
            path,
            total,
            nav: navigation,
            modes: [buttons[0], buttons[1], buttons[2]],
            map,
            table,
            notice,
            count,
            selected,
            delete,
            delete_action,
            zoom,
        }
    }
}

impl App {
    pub fn new(persist: bool) -> Self {
        let mut preferences = if persist {
            fs::read(preference_path())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        } else {
            Preferences {
                reduced: true,
                ..Preferences::default()
            }
        };
        if !(85..=150).contains(&preferences.scale) {
            preferences.scale = 100;
        }
        apply_style(&preferences);
        let icon = include_bytes!("../assets/space-lens.ico");
        unsafe {
            qt::sl_icon(icon.as_ptr(), icon.len() as i32);
        }
        let window = qt::widget(3, NULL, "", 0);
        qt::set(window, "windowTitle", "Space Lens");
        qt::int(
            window,
            "minimumWidth",
            s(800, preferences.scale as f64 / 100.),
        );
        qt::int(
            window,
            "minimumHeight",
            s(600, preferences.scale as f64 / 100.),
        );
        unsafe {
            qt::sl_resize(window, 1360, 860);
        }
        let ui = MainUi::build(window, preferences.scale as f64 / 100.);
        for (key, id) in [
            ("Ctrl+O", CHOOSE),
            ("Alt+Left", BACK),
            ("Alt+Right", FORWARD),
            ("Alt+Up", UP),
            ("F5", REFRESH),
            ("Escape", CANCEL),
            ("Ctrl+1", FOLDERS),
            ("Ctrl+2", FILES),
            ("Ctrl+3", ALL),
            ("F1", HELP),
        ] {
            qt::shortcut(window, key, id);
        }
        let mut app = Self {
            window,
            ui,
            map_ui: None,
            session: Session::default(),
            preferences,
            mode: Mode::All,
            map_mode: Mode::All,
            scale_ui: None,
            scale_preview: None,
            issues_ui: None,
            expanded: false,
            compact: false,
            persist,
            rendered: None,
            map_rendered: None,
            expanded_paths: HashSet::new(),
            shown_location: PathBuf::new(),
            deleting: None,
            last_loading: Instant::now(),
            scan_started: Instant::now(),
            pending_origin: None,
            started: Instant::now(),
        };
        app.render();
        app
    }
    pub fn start(&mut self, path: &Path, refresh: bool) {
        self.scan_started = Instant::now();
        if !refresh {
            self.expanded_paths.clear();
        }
        self.session.start(path, refresh);
        self.render();
    }
    fn save(&self) {
        if self.persist {
            let path = preference_path();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(bytes) = serde_json::to_vec_pretty(&self.preferences) {
                let _ = fs::write(path, bytes);
            }
        }
    }
    fn scale(&self) -> f64 {
        self.preferences.scale as f64 / 100.
    }
    fn handle(&mut self, event: Event) -> bool {
        let busy = self.deleting.is_some();
        match event.id {
            CHOOSE if !busy => {
                if let Some(path) = qt::choose(self.window) {
                    self.start(&path, false);
                }
            }
            REFRESH if !busy && !self.session.root.as_os_str().is_empty() => {
                self.start(&self.session.root.clone(), true)
            }
            CANCEL => {
                if self.session.state == State::Loading {
                    self.session.cancel();
                }
            }
            BACK if !busy => self.session.back(),
            FORWARD if !busy => self.session.forward(),
            UP if !busy => self.session.up(),
            ALL => self.mode = Mode::All,
            FOLDERS => self.mode = Mode::Folders,
            FILES => self.mode = Mode::Files,
            LIGHT | DARK => {
                if self.preferences.dark != (event.id == DARK) {
                    self.preferences.dark = event.id == DARK;
                    if !self.preferences.reduced {
                        unsafe {
                            qt::sl_snapshot(self.ui.central, 150);
                        }
                    }
                    apply_style(&self.preferences);
                    self.save();
                    self.update_icons();
                }
            }
            SCALE => self.open_scale(),
            SLIDER => {
                self.scale_preview = Some((event.a as u32).clamp(85, 150));
                if let Some(scale) = &self.scale_ui {
                    unsafe {
                        qt::sl_range(scale.percent, 85, 150, event.a as i32);
                    }
                }
                return false;
            }
            19 => {
                EVENTS.with(|events| {
                    events.borrow_mut().push(Event {
                        id: PERCENT,
                        a: 100,
                        b: 0,
                        x: 0.,
                        y: 0.,
                        delta: 0.,
                        text: String::new(),
                    })
                });
                return false;
            }
            20 => {
                if let Some(scale) = &self.scale_ui {
                    unsafe {
                        qt::sl_hide(scale.dialog);
                    }
                }
            }
            18 => {
                if self.session.state == State::Loading {
                    self.session.cancel();
                } else if self.session.state == State::Empty {
                    if let Some(path) = qt::choose(self.window) {
                        self.start(&path, false);
                    }
                } else if !self.session.root.as_os_str().is_empty() {
                    self.start(&self.session.root.clone(), true);
                }
            }
            PERCENT => {
                self.scale_preview = None;
                let scale = (event.a as u32).clamp(85, 150);
                if scale != self.preferences.scale {
                    self.preferences.scale = scale;
                    apply_style(&self.preferences);
                    self.save();
                    self.close_map();
                    if let Some(map) = self.map_ui.take() {
                        canvas::bind(1, NULL);
                        unsafe {
                            qt::sl_destroy(map.dialog);
                        }
                    }
                    self.ui = MainUi::build(self.window, self.scale());
                    self.compact = false;
                    self.rendered = None;
                    self.shown_location.clear();
                    qt::int(self.window, "minimumWidth", s(800, self.scale()));
                    qt::int(self.window, "minimumHeight", s(600, self.scale()));
                }
            }
            REDUCED => {
                self.preferences.reduced = event.a != 0;
                canvas::configure(
                    self.preferences.dark,
                    self.scale(),
                    self.preferences.reduced,
                );
                self.save();
            }
            COLLAPSE => {
                unsafe {
                    qt::sl_collapse(self.ui.tree);
                }
                self.expanded_paths.clear();
            }
            HELP => unsafe { qt::sl_shortcuts(self.owner(), self.scale()) },
            ISSUES => self.show_issues(),
            ISSUES_CLOSE => {
                if let Some(dialog) = self.issues_ui {
                    unsafe {
                        qt::sl_hide(dialog);
                    }
                }
            }
            31 if !busy => {
                if let Some(path) = models::tree_path(event.a as u64) {
                    self.session.navigate(&path, true);
                }
            }
            32 => {
                if let Some(path) = models::tree_path(event.a as u64) {
                    if event.b != 0 {
                        self.expanded_paths.insert(path);
                    } else {
                        self.expanded_paths.remove(&path);
                    }
                }
            }
            41 | 71 if !busy => {
                if let Some(entry) =
                    models::entry(if event.id == 71 { 2 } else { 1 }, event.a as i32)
                {
                    if entry.is_dir {
                        self.session.navigate(&entry.path, true);
                    }
                }
            }
            MAIN_SELECT | MAP_SELECT => {}
            MAIN_COPY | MAP_COPY => {
                let model = if event.id == MAP_COPY { 2 } else { 1 };
                let table = if model == 2 {
                    self.map_ui.as_ref().map_or(self.ui.table, |map| map.table)
                } else {
                    self.ui.table
                };
                if let Some(entry) = models::entry(model, unsafe { qt::sl_row(table) }) {
                    unsafe {
                        qt::sl_clipboard(qt::c(&entry.path.to_string_lossy()).as_ptr());
                    }
                }
            }
            MAIN_DELETE | MAP_DELETE if !busy => self.delete_selected(event.id == MAP_DELETE),
            EXPAND if !busy && self.session.state == State::Ready => self.open_map(),
            MAP_CLOSE | 76 if !busy => self.close_map(),
            MAP_ALL => self.map_mode = Mode::All,
            MAP_FOLDERS => self.map_mode = Mode::Folders,
            MAP_FILES => self.map_mode = Mode::Files,
            MAP_OUT => canvas::zoom(1, 1. / 1.3, false),
            MAP_IN => canvas::zoom(1, 1.3, false),
            MAP_FIT => canvas::zoom(1, 1., true),
            MAIN_FIT => canvas::zoom(0, 1., true),
            100..=103 => {
                let mut changed = false;
                if !busy {
                    if let Some((entry, origin, table_row)) = canvas::input(
                        (event.id - 100) as usize,
                        event.a,
                        event.b,
                        event.x,
                        event.y,
                        event.delta,
                    ) {
                        if entry.path.as_os_str().is_empty() {
                            self.open_map();
                        } else if entry.is_dir {
                            self.pending_origin = if event.id == 101 { origin } else { None };
                            changed = self.session.navigate(&entry.path, true);
                        } else {
                            if let Some(row) = table_row {
                                let table = if event.id == 101 {
                                    self.map_ui.as_ref().map_or(self.ui.table, |map| map.table)
                                } else {
                                    self.ui.table
                                };
                                unsafe {
                                    qt::sl_select_row(table, row);
                                }
                            }
                        }
                    }
                }
                if event.a == 5 {
                    self.zoom_labels();
                }
                return changed;
            }
            998 => self.responsive(),
            999 => self.save(),
            _ => return false,
        }
        true
    }
    fn owner(&self) -> W {
        if self.expanded {
            self.map_ui.as_ref().map_or(self.window, |map| map.dialog)
        } else {
            self.window
        }
    }
    fn update_icons(&self) {
        for (i, button) in self.ui.nav.iter().enumerate() {
            unsafe {
                qt::sl_navigation_icon(*button, i as i32, self.scale());
            }
        }
        if let Some(map) = &self.map_ui {
            for (i, button) in map.nav.iter().enumerate() {
                unsafe {
                    qt::sl_navigation_icon(*button, i as i32, self.scale());
                }
            }
        }
    }
    fn responsive(&mut self) {
        let narrow = unsafe { qt::sl_width(self.window) } < s(1120, self.scale());
        qt::visible(self.ui.sidebar, !narrow);
        if narrow != self.compact {
            self.compact = narrow;
            unsafe {
                if narrow {
                    qt::sl_tab_add(
                        self.ui.tabs,
                        self.ui.overview_scroll,
                        qt::c("Space overview").as_ptr(),
                    );
                    qt::sl_tab_add(
                        self.ui.tabs,
                        self.ui.table_panel,
                        qt::c("Breakdown").as_ptr(),
                    );
                } else {
                    qt::sl_tab_clear(self.ui.tabs);
                    qt::sl_split_add(self.ui.content, self.ui.overview_scroll);
                    qt::sl_split_add(self.ui.content, self.ui.table_panel);
                    qt::sl_split_sizes(self.ui.content, 340, 620);
                    qt::sl_show(self.ui.overview_scroll, 0);
                    qt::sl_show(self.ui.table_panel, 0);
                }
            }
        }
        qt::int(
            self.ui.overview_scroll,
            "minimumWidth",
            if narrow {
                0
            } else {
                (unsafe { qt::sl_hint_width(self.ui.overview) }) + s(12, self.scale())
            },
        );
        let tall = unsafe { qt::sl_height(self.window) } >= s(840, self.scale());
        qt::int(
            self.ui.rank_scroll,
            "minimumHeight",
            s(
                if narrow {
                    72
                } else if tall {
                    184
                } else {
                    108
                },
                self.scale(),
            ),
        );
        qt::int(
            self.ui.map,
            "minimumHeight",
            s(if narrow { 100 } else { 150 }, self.scale()),
        );
    }
    pub fn render(&mut self) {
        self.responsive();
        let ready = self.session.state == State::Ready;
        let loading = self.session.state == State::Loading;
        let busy = self.deleting.is_some();
        qt::text(
            self.ui.path,
            self.session
                .current
                .to_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("No folder selected"),
        );
        qt::set(self.window, "sl_root", &self.session.root.to_string_lossy());
        qt::text(self.ui.scale, &format!("Scale {}%", self.preferences.scale));
        qt::boolean(self.ui.themes[0], "checked", !self.preferences.dark);
        qt::boolean(self.ui.themes[1], "checked", self.preferences.dark);
        for (i, button) in self.ui.modes.iter().enumerate() {
            qt::boolean(
                *button,
                "checked",
                self.mode == [Mode::All, Mode::Folders, Mode::Files][i],
            );
        }
        qt::enabled(
            self.ui.nav[0],
            ready && !busy && self.session.history_index > 0,
        );
        qt::enabled(
            self.ui.nav[1],
            ready && !busy && self.session.history_index + 1 < self.session.history.len(),
        );
        qt::enabled(
            self.ui.nav[2],
            ready && !busy && self.session.current != self.session.root,
        );
        for widget in [self.ui.choose, self.ui.tree, self.ui.table] {
            qt::enabled(widget, !busy);
        }
        qt::enabled(
            self.ui.refresh,
            !busy && !self.session.root.as_os_str().is_empty(),
        );
        qt::enabled(self.ui.collapse, ready && !busy);
        qt::enabled(self.ui.expand, ready && !busy);
        if let Some(scale) = &self.scale_ui {
            unsafe {
                qt::sl_range(
                    scale.slider,
                    85,
                    150,
                    self.scale_preview.unwrap_or(self.preferences.scale) as i32,
                );
                qt::sl_range(
                    scale.percent,
                    85,
                    150,
                    self.scale_preview.unwrap_or(self.preferences.scale) as i32,
                );
            }
            qt::boolean(scale.reduced, "checked", self.preferences.reduced);
        }
        let key = (
            self.session.generation,
            self.session.current.clone(),
            self.mode,
            self.session.state,
        );
        let rebuild = self.rendered.as_ref() != Some(&key);
        self.rendered = Some(key);
        let map_key = (
            self.session.generation,
            self.session.current.clone(),
            self.map_mode,
        );
        let map_rebuild = self.expanded && self.map_rendered.as_ref() != Some(&map_key);
        if map_rebuild {
            self.map_rendered = Some(map_key);
        }

        if ready {
            if let Some(snapshot) = self.session.snapshot.clone() {
                let dir = *snapshot.index.get(&self.session.current).unwrap_or(&0);
                let listing = &snapshot.listings[dir];
                let partial = listing.errors > 0;
                let tree_changed = if rebuild || map_rebuild {
                    models::publish(
                        Arc::clone(&snapshot),
                        &self.session.current,
                        self.mode,
                        self.map_mode,
                    )
                } else {
                    false
                };
                if rebuild {
                    if tree_changed || self.shown_location.as_os_str().is_empty() {
                        unsafe {
                            qt::sl_model_reset(self.ui.tree);
                        }
                        let root = snapshot.root.clone();
                        let node = models::tree_node(&root).unwrap_or(1);
                        unsafe {
                            qt::sl_tree_expand(self.ui.tree, node, 1);
                        }
                        for path in self.expanded_paths.clone() {
                            if let Some(node) = models::tree_node(&path) {
                                unsafe {
                                    qt::sl_tree_expand(self.ui.tree, node, 1);
                                }
                            }
                        }
                    }
                    unsafe {
                        qt::sl_model_reset(self.ui.table);
                    }
                    let entries = models::view(1);
                    canvas::set_entries(0, entries.subset(true, 20, true), None);
                    canvas::set_entries(2, entries.subset(false, 6, false), None);
                    qt::int(
                        self.ui.ranks,
                        "minimumHeight",
                        s((entries.len().min(6) as i32 * 64 - 8).max(72), self.scale()),
                    );
                    let positive = entries
                        .iter()
                        .filter(|e| e.size.is_some_and(|n| n > 0))
                        .count();
                    qt::text(
                        self.ui.map_caption,
                        &if positive > 20 {
                            "Largest 20 + Other".into()
                        } else {
                            format!(
                                "{} measured {}",
                                commas(positive as u64),
                                match self.mode {
                                    Mode::All => "items",
                                    Mode::Folders => "folders",
                                    Mode::Files => "files",
                                }
                            )
                        },
                    );
                    qt::text(
                        self.ui.count,
                        &format!(
                            "All {} {}",
                            commas(entries.len() as u64),
                            match self.mode {
                                Mode::All => "items",
                                Mode::Folders => "folders",
                                Mode::Files => "files",
                            }
                        ),
                    );
                    qt::text(
                        self.ui.table_empty,
                        if !listing.error.is_empty() {
                            &listing.error
                        } else {
                            match self.mode {
                                Mode::All => "No items in this folder.",
                                Mode::Folders => "No folders in this folder.",
                                Mode::Files => "No files in this folder.",
                            }
                        },
                    );
                    unsafe {
                        qt::sl_stack_set(
                            self.ui.table_stack,
                            if entries.is_empty() {
                                self.ui.table_empty
                            } else {
                                self.ui.table
                            },
                        );
                    }
                    if self.session.current != self.shown_location {
                        self.shown_location = self.session.current.clone();
                        if let Some(node) = models::tree_node(&self.session.current) {
                            let mut path = self.session.current.clone();
                            while let Some(parent) = path.parent() {
                                if !crate::scanner::within(parent, &self.session.root) {
                                    break;
                                }
                                if let Some(node) = models::tree_node(parent) {
                                    unsafe {
                                        qt::sl_tree_expand(self.ui.tree, node, 1);
                                    }
                                }
                                path = parent.to_path_buf();
                            }
                            unsafe {
                                qt::sl_tree_select(self.ui.tree, node);
                            }
                        }
                    }
                }
                if map_rebuild {
                    let origin = self.pending_origin.take();
                    if let Some(map) = &self.map_ui {
                        unsafe {
                            qt::sl_model_reset(map.table);
                            if origin.is_some() && !self.preferences.reduced {
                                qt::sl_snapshot(map.map, 180);
                            }
                        }
                    }
                    canvas::set_entries(1, models::view(2).subset(true, usize::MAX, false), origin);
                }
                qt::text(
                    self.ui.total,
                    &(if !listing.error.is_empty() && listing.entries.is_empty() {
                        "Unknown".into()
                    } else {
                        size_text(Some(listing.total))
                    } + if partial { " +" } else { "" }),
                );
                qt::text(
                    self.ui.summary,
                    &format!(
                        "{} direct items · Folder total{}",
                        commas(listing.entries.len() as u64),
                        if partial {
                            " · Partial measurement"
                        } else {
                            ""
                        }
                    ),
                );
                qt::text(
                    self.ui.stamp,
                    &format!("Measured {}", measured_time(snapshot.measured_at)),
                );
                let notice = if busy {
                    "Windows is handling deletion. Finish or cancel in the Windows dialog.".into()
                } else if !self.session.message.is_empty() {
                    self.session.message.clone()
                } else if partial {
                    format!(
                        "Partial scan: {} read failure(s). Sizes marked + are lower bounds; shares are unavailable.",
                        commas(listing.errors as u64)
                    )
                } else {
                    format!(
                        "Snapshot complete · {} link(s) excluded · Refresh to measure changes.",
                        commas(snapshot.links_skipped as u64)
                    )
                };
                qt::text(self.ui.notice, &notice);
                qt::visible(self.ui.issues, !snapshot.issues.is_empty());
                qt::visible(self.ui.retry, partial);
                unsafe {
                    qt::sl_stack_set(
                        self.ui.workspace,
                        if self.compact {
                            self.ui.tabs
                        } else {
                            self.ui.content
                        },
                    );
                }
                if self.expanded {
                    self.render_map(&snapshot, dir, busy);
                }
            }
        } else {
            if rebuild {
                models::clear();
                canvas::set_entries(1, Default::default(), None);
                self.map_rendered = None;
                unsafe {
                    qt::sl_model_reset(self.ui.tree);
                    qt::sl_model_reset(self.ui.table);
                }
                canvas::set_entries(0, Default::default(), None);
                canvas::set_entries(2, Default::default(), None);
            }
            qt::text(self.ui.total, if loading { "Scanning…" } else { "—" });
            qt::text(
                self.ui.summary,
                if loading {
                    "Building a complete folder snapshot"
                } else {
                    "Choose a folder to begin."
                },
            );
            qt::text(self.ui.notice, &self.session.message);
            qt::visible(self.ui.issues, false);
            qt::visible(self.ui.retry, false);
            let (title, message, action) = match self.session.state {
                State::Loading => (
                    "Measuring your folder",
                    "Building a complete snapshot. You can cancel at any time.",
                    "Cancel scan",
                ),
                State::Error => (
                    "This folder couldn’t be read",
                    "Restore access, then retry the scan or choose another folder.",
                    "Retry scan",
                ),
                State::Cancelled => (
                    "Scan cancelled",
                    "Retry this scan or choose a different folder.",
                    "Retry scan",
                ),
                _ => (
                    "See where your space goes",
                    "Choose a folder to explore its files, folders, and size breakdown.",
                    "Choose folder",
                ),
            };
            qt::text(self.ui.loading_title, title);
            qt::text(self.ui.loading_body, message);
            qt::text(self.ui.loading_cancel, action);
            qt::set(self.ui.loading_cancel, "accessibleName", action);
            let compact = unsafe { qt::sl_height(self.ui.loading) } < s(350, self.scale());
            qt::visible(
                self.ui.loading_glyph,
                !compact && matches!(self.session.state, State::Empty | State::Loading),
            );
            qt::visible(self.ui.loading_body, !compact);
            qt::visible(self.ui.loading_stats, loading);
            qt::visible(self.ui.loading_elapsed, loading);
            for (widget, value) in self.ui.loading_counts.iter().zip([
                commas(self.session.progress.files as u64),
                commas(self.session.progress.folders as u64),
                size_text(Some(self.session.progress.bytes)),
            ]) {
                qt::text(*widget, &value);
            }
            let seconds = self.scan_started.elapsed().as_secs();
            qt::text(
                self.ui.loading_elapsed,
                &format!(
                    "{}:{:02} elapsed · Progress depends on folder access",
                    seconds / 60,
                    seconds % 60
                ),
            );
            unsafe {
                qt::sl_stack_set(self.ui.workspace, self.ui.loading);
            }
        }
        self.selection(false);
        if self.expanded {
            self.selection(true);
        }
        self.zoom_labels();
    }
    fn render_map(&self, snapshot: &Snapshot, dir: usize, busy: bool) {
        if let Some(map) = &self.map_ui {
            let listing = &snapshot.listings[dir];
            qt::text(map.path, &self.session.current.to_string_lossy());
            qt::set(map.path, "toolTip", &self.session.current.to_string_lossy());
            qt::text(
                map.total,
                &format!(
                    "{}{}",
                    size_text(Some(listing.total)),
                    if listing.errors > 0 {
                        " + measured"
                    } else {
                        " logical size"
                    }
                ),
            );
            qt::enabled(map.nav[0], !busy && self.session.history_index > 0);
            qt::enabled(
                map.nav[1],
                !busy && self.session.history_index + 1 < self.session.history.len(),
            );
            qt::enabled(
                map.nav[2],
                !busy && self.session.current != self.session.root,
            );
            qt::enabled(map.table, !busy);
            qt::enabled(map.map, !busy);
            for (i, button) in map.modes.iter().enumerate() {
                qt::boolean(
                    *button,
                    "checked",
                    self.map_mode == [Mode::All, Mode::Folders, Mode::Files][i],
                );
            }
            qt::text(
                map.notice,
                if busy {
                    "Windows is handling deletion. Finish or cancel in the Windows dialog."
                } else if listing.errors > 0 {
                    "Partial measurement · Unknown sizes have no map area."
                } else {
                    "Every positive measured item is represented. Zero-byte items remain in the breakdown."
                },
            );
            qt::text(
                map.count,
                &format!(
                    "Breakdown · All {} items",
                    commas(models::row_count(2) as u64)
                ),
            );
        }
    }
    fn selection(&self, expanded: bool) {
        let (table, label, button, action, model) = if expanded {
            let Some(map) = &self.map_ui else {
                return;
            };
            (map.table, map.selected, map.delete, map.delete_action, 2)
        } else {
            (
                self.ui.table,
                self.ui.selected,
                self.ui.delete,
                self.ui.delete_action,
                1,
            )
        };
        let entry = models::entry(model, unsafe { qt::sl_row(table) });
        let enabled =
            self.session.state == State::Ready && self.deleting.is_none() && entry.is_some();
        qt::enabled(button, enabled);
        qt::enabled(action, enabled);
        let text = entry.map_or_else(
            || "Enter opens a folder · Ctrl+C copies its path".into(),
            |e| {
                format!(
                    "{} · {}",
                    e.name,
                    e.size.map_or("Unknown size".into(), |n| format!(
                        "{} bytes{}",
                        commas(n),
                        if e.partial {
                            " measured · Partial"
                        } else {
                            ""
                        }
                    ))
                )
            },
        );
        qt::text(label, &text);
    }
    fn zoom_labels(&self) {
        qt::text(
            self.ui.map_zoom,
            &format!("{:0.0}%", canvas::zoom_value(0) * 100.),
        );
        if let Some(map) = &self.map_ui {
            qt::text(map.zoom, &format!("{:0.0}%", canvas::zoom_value(1) * 100.));
        }
    }
    fn open_map(&mut self) {
        if self.session.state != State::Ready || self.deleting.is_some() {
            return;
        }
        if self.map_ui.is_none() {
            self.map_ui = Some(MapUi::build(self.window, self.scale()));
        }
        self.map_mode = Mode::All;
        self.expanded = true;
        self.map_rendered = None;
        self.render();
        let map = self.map_ui.as_ref().unwrap();
        unsafe {
            qt::sl_present(map.dialog, (!self.preferences.reduced) as i32);
            qt::sl_focus(map.map);
        }
    }
    fn close_map(&mut self) {
        if self.deleting.is_some() {
            return;
        }
        self.expanded = false;
        canvas::pause(1);
        if let Some(map) = &self.map_ui {
            unsafe {
                qt::sl_exit(map.dialog, if self.preferences.reduced { 0 } else { 150 });
            }
        }
    }
    fn open_scale(&mut self) {
        self.scale_preview = None;
        if self.scale_ui.is_none() {
            let dialog = qt::widget(4, self.window, "", 0);
            qt::set(dialog, "windowTitle", "Appearance");
            unsafe {
                qt::sl_dialog(dialog, 2);
            }
            let body = layout(dialog, false, 20, 14, self.scale());
            qt::add(body, label(dialog, "Interface size", "section", false), 0);
            let row = layout(NULL, true, 0, 8, self.scale());
            let slider = qt::widget(12, dialog, "", SLIDER);
            let percent = qt::widget(13, dialog, "", PERCENT);
            qt::set(slider, "accessibleName", "Interface scale slider");
            qt::set(percent, "accessibleName", "Interface scale percentage");
            qt::int(slider, "minimumWidth", s(180, self.scale()));
            qt::int(percent, "minimumWidth", s(80, self.scale()));
            qt::add(row, slider, 1);
            qt::add(row, percent, 0);
            qt::sub(body, row, 0);
            let ends = layout(NULL, true, 0, 8, self.scale());
            qt::add(ends, label(dialog, "85%", "muted", false), 0);
            qt::stretch(ends);
            qt::add(ends, label(dialog, "150%", "muted", false), 0);
            qt::sub(body, ends, 0);
            let reduced = qt::widget(14, dialog, "", REDUCED);
            qt::text(reduced, "Reduce motion");
            qt::boolean(reduced, "checked", self.preferences.reduced);
            qt::add(body, reduced, 0);
            let bottom = layout(NULL, true, 0, 8, self.scale());
            qt::add(bottom, qt::button(dialog, "Reset to 100%", "", 19), 0);
            qt::stretch(bottom);
            qt::add(bottom, qt::button(dialog, "Done", "", 20), 0);
            qt::sub(body, bottom, 0);
            self.scale_ui = Some(ScaleUi {
                dialog,
                slider,
                percent,
                reduced,
            });
        }
        let scale = self.scale_ui.as_ref().unwrap();
        unsafe {
            qt::sl_range(scale.slider, 85, 150, self.preferences.scale as i32);
            qt::sl_range(scale.percent, 85, 150, self.preferences.scale as i32);
            qt::sl_popup(scale.dialog, self.ui.scale);
            qt::sl_focus(scale.percent);
        }
    }
    fn show_issues(&mut self) {
        let Some(snapshot) = &self.session.snapshot else {
            return;
        };
        if let Some(old) = self.issues_ui.take() {
            unsafe {
                qt::sl_destroy(old);
            }
        }
        let dialog = qt::widget(4, self.window, "", 0);
        unsafe {
            qt::sl_dialog(dialog, 0);
            qt::sl_resize(dialog, s(680, self.scale()), s(440, self.scale()));
        }
        qt::set(dialog, "windowTitle", "Scan details · Space Lens v3");
        let body = layout(dialog, false, 24, 16, self.scale());
        qt::add(
            body,
            label(
                dialog,
                "Some locations could not be measured",
                "section",
                true,
            ),
            0,
        );
        let scroll = qt::widget(8, dialog, "", 0);
        let details = label(
            NULL,
            &snapshot
                .issues
                .iter()
                .map(|(path, error)| format!("{}\n{error}", path.display()))
                .collect::<Vec<_>>()
                .join("\n\n"),
            "",
            true,
        );
        qt::int(details, "textInteractionFlags", 1 | 2);
        unsafe {
            qt::sl_scroll_child(scroll, details);
        }
        qt::add(body, scroll, 1);
        qt::add(body, qt::button(dialog, "Close", "", ISSUES_CLOSE), 0);
        qt::show(dialog);
        self.issues_ui = Some(dialog);
    }
    fn delete_selected(&mut self, expanded: bool) {
        if self.session.state != State::Ready || self.deleting.is_some() {
            return;
        }
        let table = if expanded {
            self.map_ui.as_ref().map_or(self.ui.table, |map| map.table)
        } else {
            self.ui.table
        };
        let Some(entry) = models::entry(if expanded { 2 } else { 1 }, unsafe { qt::sl_row(table) })
        else {
            return;
        };
        let identity = match native::target_identity(&entry.path, &self.session.root) {
            Ok(value) => value,
            Err(error) => {
                self.session.message = error;
                return;
            }
        };
        let root = self.session.root.clone();
        let hwnd = unsafe { qt::sl_hwnd(self.owner()) };
        let (sender, receiver) = mpsc::channel();
        self.deleting = Some(receiver);
        BUSY.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            let result = native::delete(&entry.path, &root, identity, hwnd);
            let unchanged = matches!(&result, Err(native::DeleteError::Cancelled))
                && !entry.is_dir
                && native::target_identity(&entry.path, &root).is_ok_and(|value| value == identity);
            let _ = sender.send((result, entry.path.clone(), !unchanged));
            unsafe {
                qt::sl_wake();
            }
        });
    }
}

fn measured_time(time: std::time::SystemTime) -> String {
    let epoch = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut buffer = [0i8; 9];
    unsafe {
        qt::sl_local_time(epoch, buffer.as_mut_ptr());
        CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}
fn apply_style(preferences: &Preferences) {
    let palette: HashMap<String, String> =
        serde_json::from_str(include_str!("../assets/dark-colors.json")).unwrap();
    let base = include_str!("../assets/style.qss");
    let mut css = String::new();
    let mut i = 0;
    while i < base.len() {
        let value = &base[i..];
        if preferences.dark
            && value.starts_with('#')
            && value.len() >= 7
            && value[1..7].chars().all(|c| c.is_ascii_hexdigit())
        {
            let color = &value[..7];
            css.push_str(palette.get(color).map_or(color, String::as_str));
            i += 7;
        } else {
            let ch = value.chars().next().unwrap();
            css.push(ch);
            i += ch.len_utf8();
        }
    }
    css.push_str(if preferences.dark {
        include_str!("../assets/dark_style.qss")
    } else {
        include_str!("../assets/light_style.qss")
    });
    let scale = preferences.scale as f64 / 100.;
    let mut scaled = String::new();
    let mut chars = css.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            let mut digits = String::from(ch);
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                digits.push(chars.next().unwrap());
            }
            if chars.peek() == Some(&'p') {
                let mut look = chars.clone();
                look.next();
                if look.next() == Some('x') {
                    chars.next();
                    chars.next();
                    scaled.push_str(&format!(
                        "{}px",
                        (digits.parse::<f64>().unwrap() * scale)
                            .round_ties_even()
                            .max(1.)
                    ));
                    continue;
                }
            }
            scaled.push_str(&digits);
        } else {
            scaled.push(ch);
        }
    }
    let roles = [
        (10, "#f3f7f7"),
        (0, "#131416"),
        (9, "#ffffff"),
        (16, "#f4f6f8"),
        (6, "#131416"),
        (1, "#f4f6f8"),
        (8, "#131416"),
        (12, "#e5eeee"),
        (13, "#131416"),
        (20, "#5d5f60"),
    ];
    unsafe {
        qt::sl_palette_begin();
    }
    for (role, color) in roles {
        let color = if preferences.dark {
            palette.get(color).map_or(color, String::as_str)
        } else {
            color
        };
        for group in [0, 1, 2] {
            unsafe {
                qt::sl_palette(role, group, qt::c(color).as_ptr());
            }
        }
    }
    for role in [6, 8] {
        unsafe {
            qt::sl_palette(
                role,
                1,
                qt::c(if preferences.dark {
                    "#8391a1"
                } else {
                    "#65686b"
                })
                .as_ptr(),
            );
        }
    }
    unsafe {
        qt::sl_palette_end();
        qt::sl_app_dark(preferences.dark as i32);
        qt::sl_style(qt::c(&scaled).as_ptr(), scale);
    }
    canvas::configure(preferences.dark, scale, preferences.reduced);
}

pub fn run(path: Option<PathBuf>, persist: bool) -> i32 {
    unsafe {
        qt::sl_init();
    }
    let mut app = App::new(persist);
    if let Some(path) = path {
        app.start(&path, false);
    }
    qt::show(app.window);
    APP.with(|slot| *slot.borrow_mut() = Some(app));
    unsafe {
        qt::sl_timer();
        qt::sl_exec()
    }
}
