//! Settings window (egui). Edits are kept locally until Apply, then written with
//! the same commands M HUB uses and verified by reading back.

use crate::buttons::{self, Action};
use crate::settings::{self, Config, DeviceInfo};
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, StrokeKind, Vec2};

// palette
const BG: Color32 = Color32::from_rgb(0x14, 0x16, 0x1b);
const CARD: Color32 = Color32::from_rgb(0x1d, 0x21, 0x29);
const CARD_STROKE: Color32 = Color32::from_rgb(0x2a, 0x2f, 0x3a);
const WIDGET: Color32 = Color32::from_rgb(0x2a, 0x30, 0x3c);
const WIDGET_HOVER: Color32 = Color32::from_rgb(0x34, 0x3b, 0x49);
const WIDGET_ACTIVE: Color32 = Color32::from_rgb(0x3d, 0x45, 0x55);
const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xee);
const WEAK: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa5);
const ACCENT: Color32 = Color32::from_rgb(0x5b, 0x8c, 0xff);
const GOOD: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
const WARN: Color32 = Color32::from_rgb(0xf0, 0xb4, 0x4c);
const BAD: Color32 = Color32::from_rgb(0xef, 0x5f, 0x5f);

/// DPI stage colours from M HUB's DPI page (--mc-color-n10, w37, n19, n32, n26, n29),
/// which match the mouse's DPI indicator.
const STAGE_COLORS: [Color32; 6] = [
    Color32::from_rgb(0xea, 0x5e, 0x56),
    Color32::from_rgb(0x22, 0x6b, 0xee),
    Color32::from_rgb(0x32, 0xaa, 0x70),
    Color32::from_rgb(0xf5, 0xe0, 0x25),
    Color32::from_rgb(0x00, 0xdd, 0xe4),
    Color32::from_rgb(0xb5, 0x00, 0xec),
];

const LABEL_W: f32 = 120.0;

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.text_styles = [
            (TextStyle::Heading, FontId::proportional(20.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.combo_height = 320.0;

        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::from_rgb(0x10, 0x12, 0x16);
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = stroke(1.0, Color32::WHITE);
        v.slider_trailing_fill = true;
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.window_stroke = stroke(1.0, CARD_STROKE);
        let r = CornerRadius::same(6);
        v.widgets.noninteractive.bg_stroke = stroke(1.0, CARD_STROKE);
        v.widgets.noninteractive.fg_stroke = stroke(1.0, TEXT);
        for (w, fill) in [
            (&mut v.widgets.inactive, WIDGET),
            (&mut v.widgets.hovered, WIDGET_HOVER),
            (&mut v.widgets.active, WIDGET_ACTIVE),
            (&mut v.widgets.open, WIDGET_ACTIVE),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.corner_radius = r;
            w.bg_stroke = Stroke::NONE;
            w.fg_stroke = stroke(1.5, TEXT);
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.corner_radius = r;
    });
}

struct App {
    info: Option<DeviceInfo>,
    orig: Option<Config>,
    edit: Option<Config>,
    message: String,
    error: bool,
    buttons: ButtonEditor,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    System,
    Keyboard,
    Hotkey,
}

/// State of the inline button editor (one button open at a time).
struct ButtonEditor {
    open: Option<usize>,
    tab: Tab,
    /// The hotkey being built on the Hotkey tab.
    mods: u8,
    key: u8,
    recording: bool,
}

impl ButtonEditor {
    fn new() -> Self {
        Self { open: None, tab: Tab::System, mods: buttons::CTRL, key: 0x04, recording: false }
    }

    /// Open (or close) button `i`, starting on the tab its action belongs to.
    fn toggle(&mut self, i: usize, action: Action) {
        self.recording = false;
        if self.open == Some(i) {
            self.open = None;
            return;
        }
        self.open = Some(i);
        self.tab = match action {
            Action::Key(k) => {
                self.key = k;
                Tab::Keyboard
            }
            Action::Hotkey { mods, key } => {
                (self.mods, self.key) = (mods, key);
                Tab::Hotkey
            }
            _ => Tab::System,
        };
    }
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        setup_style(&cc.egui_ctx);
        let mut app = Self { info: None, orig: None, edit: None, message: String::new(), error: false, buttons: ButtonEditor::new() };
        app.reload();
        app
    }

    fn reload(&mut self) {
        let result = crate::open_device().and_then(|mut d| {
            let info = settings::device_info(&mut d).map_err(|e| e.to_string())?;
            let cfg = settings::Config::read(&mut d).map_err(|e| e.to_string())?;
            Ok((info, cfg))
        });
        match result {
            Ok((Some(info), Some(cfg))) => {
                self.info = Some(info);
                self.orig = Some(cfg.clone());
                self.edit = Some(cfg);
                self.message.clear();
                self.error = false;
            }
            Ok(_) => self.fail("Mouse is asleep or not connected. Move it and press Refresh."),
            Err(e) => self.fail(&e),
        }
    }

    fn fail(&mut self, msg: &str) {
        self.message = msg.into();
        self.error = true;
    }

    fn apply(&mut self) {
        let (Some(edit), Some(orig)) = (&self.edit, &self.orig) else { return };
        match crate::open_device().and_then(|mut d| edit.apply(orig, &mut d)) {
            Ok(now) => {
                self.orig = Some(now.clone());
                self.edit = Some(now);
                self.message = "Saved to the mouse".into();
                self.error = false;
            }
            Err(e) => {
                // show what the mouse actually has now
                self.reload();
                self.fail(&format!("Write failed: {e}"));
            }
        }
    }
}

// ---------------------------------------------------------------- widgets

fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    card_with(ui, title, &mut (), |_, _| {}, |ui, _| body(ui));
}

/// A rounded section with a title row; `header` adds controls on the right of the
/// title. Both closures get `state` so they can edit the same thing.
fn card_with<T>(
    ui: &mut egui::Ui,
    title: &str,
    state: &mut T,
    header: impl FnOnce(&mut egui::Ui, &mut T),
    body: impl FnOnce(&mut egui::Ui, &mut T),
) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(stroke(1.0, CARD_STROKE))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(16.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| header(ui, state));
            });
            ui.add_space(4.0);
            body(ui, state);
        });
    ui.add_space(12.0);
}

/// Label on the left, control filling the rest of the row.
fn row(ui: &mut egui::Ui, label: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        // fixed-width label column so every control starts at the same x
        let (rect, _) = ui.allocate_exact_size(Vec2::new(LABEL_W, 28.0), Sense::hover());
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), WEAK);
        control(ui);
    });
}

fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = Vec2::new(40.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let off = if resp.hovered() { WIDGET_HOVER } else { WIDGET };
        let fill = egui::ecolor::tint_color_towards(off, ACCENT).lerp_to_gamma(ACCENT, t);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, if *on || t > 0.0 { fill } else { off });
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), radius - 3.0, Color32::WHITE);
    }
    resp
}

/// Row of equal-width buttons; returns true if the selection changed.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, String)]) -> bool {
    let mut changed = false;
    let gap = 4.0;
    let w = ((ui.available_width() - gap * (options.len() as f32 - 1.0)) / options.len() as f32).max(36.0);
    ui.spacing_mut().item_spacing.x = gap;
    for (v, label) in options {
        let selected = *value == *v;
        let text = RichText::new(label).color(if selected { Color32::WHITE } else { TEXT });
        let btn = egui::Button::new(text).fill(if selected { ACCENT } else { WIDGET }).min_size(Vec2::new(w, 28.0));
        if ui.add(btn).clicked() && !selected {
            *value = *v;
            changed = true;
        }
    }
    changed
}

fn battery_icon(ui: &mut egui::Ui, level: u8, charging: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 16.0), Sense::hover());
    let body = egui::Rect::from_min_size(rect.min, Vec2::new(26.0, 16.0));
    let nub = egui::Rect::from_min_size(egui::pos2(body.right() + 1.0, rect.center().y - 3.5), Vec2::new(3.0, 7.0));
    let color = if charging {
        ACCENT
    } else if level <= 15 {
        BAD
    } else if level <= 30 {
        WARN
    } else {
        GOOD
    };
    let p = ui.painter();
    p.rect_stroke(body, 3.0, stroke(1.5, WEAK), StrokeKind::Inside);
    p.rect_filled(nub, 1.0, WEAK);
    let inner = body.shrink(3.0);
    let fill = egui::Rect::from_min_size(inner.min, Vec2::new(inner.width() * level.min(100) as f32 / 100.0, inner.height()));
    p.rect_filled(fill, 1.5, color);
}

fn chip(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let label = RichText::new(text).color(if selected { Color32::WHITE } else { TEXT });
    ui.add(egui::Button::new(label).fill(if selected { ACCENT } else { WIDGET }).min_size(Vec2::new(0.0, 26.0)))
}

fn chip_group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(title).small().color(WEAK));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        body(ui);
    });
    ui.add_space(4.0);
}

/// The row header: the button's current action, clickable to open its editor.
fn action_header(ui: &mut egui::Ui, action: Action, open: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let fill = if resp.hovered() { WIDGET_HOVER } else { WIDGET };
    let border = if open { stroke(1.5, ACCENT) } else { Stroke::NONE };
    let p = ui.painter();
    p.rect(rect, 6.0, fill, border, StrokeKind::Inside);
    p.text(rect.left_center() + Vec2::new(10.0, 0.0), egui::Align2::LEFT_CENTER, action.label(), egui::FontId::proportional(14.0), TEXT);
    // the default font has no ▾, so draw the chevron
    let c = rect.right_center() - Vec2::new(16.0, 0.0);
    let (tip, base) = if open { (-3.0, 3.0) } else { (3.0, -3.0) };
    let chevron = vec![c + Vec2::new(-5.0, base), c + Vec2::new(5.0, base), c + Vec2::new(0.0, tip)];
    p.add(egui::Shape::convex_polygon(chevron, WEAK, Stroke::NONE));
    p.text(rect.right_center() - Vec2::new(30.0, 0.0), egui::Align2::RIGHT_CENTER, action.category(), egui::FontId::proportional(12.0), WEAK);
    resp
}

/// The editor under an open button: System / Keyboard / Hotkey tabs, like M HUB.
fn action_editor(ui: &mut egui::Ui, ed: &mut ButtonEditor, action: &mut Action) {
    egui::Frame::new()
        .fill(BG)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let tabs = [(Tab::System, "System".into()), (Tab::Keyboard, "Keyboard".into()), (Tab::Hotkey, "Hotkey".into())];
                if segmented(ui, &mut ed.tab, &tabs) {
                    ed.recording = false;
                }
            });
            ui.add_space(6.0);
            match ed.tab {
                Tab::System => system_tab(ui, action),
                Tab::Keyboard => keyboard_tab(ui, action),
                Tab::Hotkey => hotkey_tab(ui, ed, action),
            }
        });
}

fn system_tab(ui: &mut egui::Ui, action: &mut Action) {
    if let Action::Other(..) = action {
        ui.label(RichText::new(format!("Currently: {} (set in M HUB; kept unless you pick something else)", action.label())).small().color(WARN));
    }
    let mut pick = |ui: &mut egui::Ui, a: Action, text: &str| {
        if chip(ui, *action == a, text).clicked() {
            *action = a;
        }
    };
    chip_group(ui, "General", |ui| {
        pick(ui, Action::Default, "Default");
        pick(ui, Action::Disabled, "Disabled");
    });
    chip_group(ui, "Mouse", |ui| {
        for (v, n) in buttons::MOUSE {
            pick(ui, Action::Mouse(v), n);
        }
    });
    chip_group(ui, "Media", |ui| {
        for (v, n) in buttons::MEDIA {
            pick(ui, Action::Media(v), n);
        }
    });
    chip_group(ui, "DPI", |ui| {
        for (v, n) in buttons::DPI {
            pick(ui, Action::Dpi(v), n);
        }
    });
    chip_group(ui, "System", |ui| {
        for (v, n) in buttons::SYSTEM {
            pick(ui, Action::System(v), n);
        }
        for (mods, key, n) in buttons::HOTKEY_PRESETS {
            pick(ui, Action::Hotkey { mods, key }, n);
        }
    });
}

fn keyboard_tab(ui: &mut egui::Ui, action: &mut Action) {
    for (group, keys) in buttons::key_groups() {
        chip_group(ui, group, |ui| {
            for k in keys {
                if chip(ui, *action == Action::Key(k), &buttons::key_name(k)).clicked() {
                    *action = Action::Key(k);
                }
            }
        });
    }
}

fn hotkey_tab(ui: &mut egui::Ui, ed: &mut ButtonEditor, action: &mut Action) {
    let mut changed = false;
    if ed.recording {
        // take the next key press (with whatever modifiers are held) as the hotkey
        let pressed = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((key.name(), *modifiers)),
                _ => None,
            })
        });
        if let Some((name, m)) = pressed {
            if name == "Escape" && !m.any() {
                ed.recording = false;
            } else if let Some(k) = buttons::hid_from_key_name(name) {
                ed.key = k;
                ed.mods = (if m.ctrl { buttons::CTRL } else { 0 })
                    | (if m.shift { buttons::SHIFT } else { 0 })
                    | (if m.alt { buttons::ALT } else { 0 });
                ed.recording = false;
                changed = true;
            }
        }
        ui.ctx().request_repaint();
    }

    chip_group(ui, "Modifiers", |ui| {
        for (bit, name) in [(buttons::CTRL, "Ctrl"), (buttons::SHIFT, "Shift"), (buttons::ALT, "Alt"), (buttons::SUPER, "Super")] {
            if chip(ui, ed.mods & bit != 0, name).clicked() {
                ed.mods ^= bit;
                changed = true;
            }
        }
    });
    ui.label(RichText::new("Key").small().color(WEAK));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("hotkey-key")
            .width(150.0)
            .height(360.0)
            .selected_text(buttons::key_name(ed.key))
            .show_ui(ui, |ui| {
                for (group, keys) in buttons::key_groups() {
                    ui.label(RichText::new(group).small().color(WEAK));
                    for k in keys {
                        if ui.selectable_label(ed.key == k, buttons::key_name(k)).clicked() {
                            ed.key = k;
                            changed = true;
                        }
                    }
                }
            });
        let record = if ed.recording { "Press keys… (Esc cancels)" } else { "● Record" };
        if chip(ui, ed.recording, record).clicked() {
            ed.recording = !ed.recording;
        }
    });
    ui.add_space(4.0);
    if changed {
        // no modifier is just a plain key
        *action = if ed.mods == 0 { Action::Key(ed.key) } else { Action::Hotkey { mods: ed.mods, key: ed.key } };
    }
    let current = match action {
        Action::Hotkey { mods, key } => buttons::combo_name(*mods, *key),
        _ => "pick modifiers and a key, or record one".into(),
    };
    ui.label(RichText::new(format!("Sends: {current}")).color(if matches!(action, Action::Hotkey { .. }) { TEXT } else { WEAK }));
}

/// The DPI stages as a grid of tiles: click a tile to make it active, drag or
/// click the number to edit it.
fn dpi_tiles(ui: &mut egui::Ui, cfg: &mut Config, usb: bool) {
    let stages = cfg.stages();
    let cols = 3;
    let gap = 8.0;
    let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
    let h = 64.0;
    for row_start in (0..stages).step_by(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for s in row_start..(row_start + cols).min(stages) {
                let active = cfg.active_stage(usb) as usize == s;
                let color = STAGE_COLORS[s];
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
                let (fill, border) = if active {
                    (WIDGET.lerp_to_gamma(color, 0.22), stroke(1.5, color))
                } else if resp.hovered() {
                    (WIDGET_HOVER, stroke(1.0, CARD_STROKE))
                } else {
                    (WIDGET, Stroke::NONE)
                };
                let p = ui.painter();
                p.rect(rect, 10.0, fill, border, StrokeKind::Inside);
                // colour strip along the left edge, like M HUB's stage swatches
                let strip = egui::Rect::from_min_size(rect.left_top() + Vec2::new(5.0, 10.0), Vec2::new(4.0, h - 20.0));
                p.rect_filled(strip, 2.0, color);
                p.circle_filled(rect.left_top() + Vec2::new(20.0, 16.0), 4.0, color);
                p.text(
                    rect.left_top() + Vec2::new(30.0, 9.0),
                    egui::Align2::LEFT_TOP,
                    if active { format!("Stage {} · active", s + 1) } else { format!("Stage {}", s + 1) },
                    egui::FontId::proportional(12.0),
                    if active { TEXT } else { WEAK },
                );
                let mut dpi = cfg.dpi(s);
                let value_rect = egui::Rect::from_min_size(rect.left_top() + Vec2::new(12.0, 28.0), Vec2::new(w - 18.0, 30.0));
                let dv = ui.put(
                    value_rect,
                    |ui: &mut egui::Ui| {
                        // borderless big number
                        let wv = &mut ui.visuals_mut().widgets;
                        for st in [&mut wv.inactive, &mut wv.hovered] {
                            st.weak_bg_fill = Color32::TRANSPARENT;
                            st.bg_fill = Color32::TRANSPARENT;
                        }
                        ui.style_mut().override_font_id = Some(egui::FontId::proportional(20.0));
                        ui.add(
                            egui::DragValue::new(&mut dpi)
                                .range(settings::DPI_MIN..=settings::DPI_MAX)
                                .speed(10.0),
                        )
                    },
                );
                if dv.changed() {
                    cfg.set_dpi(s, (dpi + 25) / 50 * 50);
                }
                if resp.clicked() {
                    cfg.set_active_stage(s as u8);
                }
            }
        });
    }
}

// ---------------------------------------------------------------- window

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dirty = self.edit.is_some() && self.edit != self.orig;

        egui::TopBottomPanel::bottom("actions")
            .frame(egui::Frame::new().fill(CARD).inner_margin(Margin::symmetric(16, 12)).stroke(stroke(1.0, CARD_STROKE)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (color, text) = if self.error {
                        (BAD, self.message.as_str())
                    } else if dirty {
                        (WARN, "● Unsaved changes")
                    } else {
                        (WEAK, self.message.as_str())
                    };
                    ui.label(RichText::new(text).color(color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let apply = egui::Button::new(RichText::new("Apply").color(Color32::WHITE).strong())
                            .fill(if dirty { ACCENT } else { WIDGET })
                            .min_size(Vec2::new(90.0, 32.0));
                        if ui.add_enabled(dirty, apply).clicked() {
                            self.apply();
                        }
                        if ui.add_enabled(dirty, egui::Button::new("Revert").min_size(Vec2::new(80.0, 32.0))).clicked() {
                            self.edit = self.orig.clone();
                            self.message.clear();
                        }
                    });
                });
            });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(16))).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                // keep the scrollbar off the cards
                ui.set_max_width(ui.available_width() - 14.0);
                // header
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("MCHOSE L7 Pro").size(22.0).strong());
                        if let Some(i) = &self.info {
                            let link = if i.wired { "USB cable" } else { "2.4G receiver" };
                            ui.label(RichText::new(format!("{link}  ·  firmware {}", i.firmware)).color(WEAK));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Refresh").clicked() {
                            self.reload();
                        }
                        if let Some(i) = &self.info {
                            ui.add_space(6.0);
                            let label = if i.charging { format!("{}% · charging", i.battery) } else { format!("{}%", i.battery) };
                            ui.label(RichText::new(label).strong());
                            battery_icon(ui, i.battery, i.charging);
                        }
                    });
                });
                ui.add_space(14.0);

                let usb = self.info.as_ref().is_some_and(|i| i.wired);
                let Some(cfg) = self.edit.as_mut() else {
                    card(ui, "Not connected", |ui| {
                        ui.label(RichText::new(&self.message).color(WEAK));
                    });
                    return;
                };

                let ed = &mut self.buttons;
                card(ui, "Buttons", |ui| {
                    row(ui, settings::BUTTONS[0], |ui| {
                        ui.label(RichText::new("Left click (fixed)").color(WEAK));
                    });
                    for i in 1..5 {
                        let mut action = cfg.button(i);
                        let open = ed.open == Some(i);
                        row(ui, settings::BUTTONS[i], |ui| {
                            if action_header(ui, action, open).clicked() {
                                ed.toggle(i, action);
                            }
                        });
                        if ed.open == Some(i) {
                            action_editor(ui, ed, &mut action);
                            ui.add_space(4.0);
                        }
                        if action != cfg.button(i) {
                            cfg.set_button(i, action);
                        }
                    }
                    row(ui, settings::BUTTONS[5], |ui| {
                        ui.label(RichText::new("DPI cycle (fixed)").color(WEAK));
                    });
                });
                card_with(
                    ui,
                    "DPI",
                    cfg,
                    |ui, cfg| {
                        let n = cfg.stages();
                        if ui.add_enabled(n < 6, egui::Button::new("+").min_size(Vec2::splat(26.0))).clicked() {
                            cfg.set_stages(n + 1);
                        }
                        ui.label(format!("{n} stages"));
                        if ui.add_enabled(n > 1, egui::Button::new("−").min_size(Vec2::splat(26.0))).clicked() {
                            cfg.set_stages(n - 1);
                            if cfg.active_stage(usb) as usize >= n - 1 {
                                cfg.set_active_stage(n as u8 - 2);
                            }
                        }
                    },
                    |ui, cfg| {
                        dpi_tiles(ui, cfg, usb);
                        ui.label(RichText::new("Click a stage to make it active · drag or click the number to edit").small().color(WEAK));
                    },
                );

                card(ui, "Performance", |ui| {
                    let link = if usb { "USB" } else { "2.4G" };
                    ui.label(RichText::new(format!("Polling rate ({link})")).color(WEAK));
                    let mut r = cfg.rate(usb);
                    let opts: Vec<(u8, String)> = settings::RATES
                        .iter()
                        .enumerate()
                        .map(|(i, hz)| (i as u8, if *hz >= 1000 { format!("{}K", hz / 1000) } else { hz.to_string() }))
                        .collect();
                    ui.horizontal(|ui| {
                        if segmented(ui, &mut r, &opts) {
                            cfg.set_rate(usb, r);
                        }
                    });
                    ui.add_space(6.0);

                    ui.label(RichText::new("Sensor mode").color(WEAK));
                    let mut m = cfg.sensor_mode();
                    let modes: Vec<(u8, String)> =
                        settings::SENSOR_MODES.iter().enumerate().map(|(i, n)| (i as u8, n.to_string())).collect();
                    ui.horizontal(|ui| {
                        if segmented(ui, &mut m, &modes) {
                            cfg.set_sensor_mode(m);
                        }
                    });
                    ui.add_space(6.0);

                    row(ui, "Lift-off distance", |ui| {
                        let mut lod = cfg.lod();
                        if segmented(ui, &mut lod, &[(1, "1 mm".into()), (2, "2 mm".into())]) {
                            cfg.set_lod(lod);
                        }
                    });
                    for (bit, name) in [(0x04, "Ripple control"), (0x08, "Angle snapping"), (0x10, "Motion sync")] {
                        row(ui, name, |ui| {
                            let mut on = cfg.flag(bit);
                            if toggle(ui, &mut on).changed() {
                                cfg.set_flag(bit, on);
                            }
                        });
                    }

                    ui.add_space(4.0);
                    let slider_w = |ui: &mut egui::Ui| ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(80.0);
                    row(ui, "Sensor angle", |ui| {
                        slider_w(ui);
                        let mut a = cfg.angle();
                        if ui.add(egui::Slider::new(&mut a, -settings::ANGLE_MAX..=settings::ANGLE_MAX).suffix("°")).changed() {
                            cfg.set_angle(a);
                        }
                    });
                    row(ui, "Button debounce", |ui| {
                        slider_w(ui);
                        let mut d = cfg.debounce();
                        if ui.add(egui::Slider::new(&mut d, 0..=settings::DEBOUNCE_MAX).suffix(" ms")).changed() {
                            cfg.set_debounce(d);
                        }
                    });
                    row(ui, "Sleep after", |ui| {
                        slider_w(ui);
                        let mut s = cfg.sleep();
                        let slider = egui::Slider::new(&mut s, 0..=settings::SLEEP_MAX)
                            .custom_formatter(|v, _| if v == 0.0 { "never".into() } else { format!("{v} min") });
                        if ui.add(slider).changed() {
                            cfg.set_sleep(s);
                        }
                    });
                });

                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("OpenL7 is unofficial and not affiliated with MCHOSE. Use at your own risk.")
                            .small()
                            .color(WEAK),
                    );
                });
            });
        });
    }
}

fn runtime_path(name: &str) -> String {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    format!("{dir}/{name}")
}

/// Only one settings window: the first one holds a flock and listens on a socket;
/// a second launch pokes that socket (so the window comes to the front) and exits.
fn single_instance() -> Option<(std::fs::File, std::os::unix::net::UnixDatagram)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixDatagram;
    let sock_path = runtime_path("openl7-gui.sock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(runtime_path("openl7-gui.lock"))
        .ok()?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        if let Ok(s) = UnixDatagram::unbound() {
            let _ = s.send_to(b"show", &sock_path);
        }
        return None;
    }
    let _ = std::fs::remove_file(&sock_path);
    let sock = UnixDatagram::bind(&sock_path).ok()?;
    Some((lock, sock))
}

pub fn run() -> Result<(), String> {
    let Some((_lock, sock)) = single_instance() else { return Ok(()) };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenL7")
            .with_app_id("openl7")
            .with_inner_size([480.0, 860.0])
            .with_min_inner_size([420.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OpenL7",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 16];
                while sock.recv(&mut buf).is_ok() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                }
            });
            Ok(Box::new(App::new(cc)))
        }),
    )
    .map_err(|e| e.to_string())
}
