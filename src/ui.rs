use crate::app::*;
use crate::data::*;
use crate::icons::{self, Icon};
use egui::{Color32, RichText, Vec2};

const BG: Color32 = Color32::from_rgb(14, 16, 22);
const PANEL: Color32 = Color32::from_rgb(20, 23, 31);
const ACCENT: Color32 = Color32::from_rgb(99, 102, 241);
const MUTED: Color32 = Color32::from_rgb(140, 148, 166);

pub fn style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "julius".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/JuliusSansOne-Regular.ttf"))),
    );
    // Julius Sans One first; the default fonts remain as fallback for missing glyphs.
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "julius".into());
    ctx.set_fonts(fonts);
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(24, 28, 38);
    v.extreme_bg_color = BG;
    v.selection.bg_fill = ACCENT;
    v.widgets.noninteractive.bg_stroke.color = Color32::from_rgb(34, 38, 50);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.interaction.selectable_labels = false;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
    });
}

fn paint_star_icon(p: &egui::Painter, rect: egui::Rect, filled: bool) {
    let (icon, col) = if filled { (Icon::Star, Color32::from_rgb(250, 204, 21)) } else { (Icon::StarOutline, MUTED) };
    icons::paint(p, icon, rect, col);
}

fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn fmt_clock(ms: i64) -> String {
    let secs = ms / 1000;
    let local = secs - tz_offset_secs();
    let _ = local;
    let t = (secs + tz_offset_secs()).rem_euclid(86400);
    format!("{:02}:{:02}", t / 3600, t / 60 % 60)
}

fn tz_offset_secs() -> i64 {
    use std::sync::OnceLock;
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        #[repr(C)]
        struct Tzi {
            bias: i32,
            sn: [u16; 32],
            sd: [u16; 8],
            sb: i32,
            dn: [u16; 32],
            dd: [u16; 8],
            db: i32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetTimeZoneInformation(t: *mut Tzi) -> u32;
        }
        let mut t: Tzi = unsafe { std::mem::zeroed() };
        let r = unsafe { GetTimeZoneInformation(&mut t) };
        let bias = t.bias + if r == 2 { t.db } else { t.sb };
        -(bias as i64) * 60
    })
}

fn tile_rects(r: egui::Rect, n: usize, layout: MultiLayout, active: usize) -> Vec<egui::Rect> {
    let gap = 2.0;
    let (cols, rows) = match layout {
        MultiLayout::Columns => (n, 1),
        MultiLayout::Rows => (1, n),
        _ => {
            let c = if n <= 1 { 1 } else { 2 };
            (c, n.div_ceil(c))
        }
    };
    if layout == MultiLayout::Focus && n > 1 {
        // Active tile takes ~70% of the width; the rest are stacked on the right.
        let big_w = (r.width() - gap) * 0.7;
        let side = egui::Rect::from_min_max(egui::pos2(r.min.x + big_w + gap, r.min.y), r.max);
        let small = tile_rects(side, n - 1, MultiLayout::Rows, 0);
        let mut it = small.into_iter();
        return (0..n)
            .map(|i| if i == active { egui::Rect::from_min_size(r.min, Vec2::new(big_w, r.height())) } else { it.next().unwrap() })
            .collect();
    }
    let (w, h) = ((r.width() - gap * (cols - 1) as f32) / cols as f32, (r.height() - gap * (rows - 1) as f32) / rows as f32);
    (0..n).map(|i| egui::Rect::from_min_size(r.min + Vec2::new((i % cols) as f32 * (w + gap), (i / cols) as f32 * (h + gap)), Vec2::new(w, h))).collect()
}

fn res_label(w: i64, h: i64) -> String {
    let q = if h >= 2000 || w >= 3600 { "4K" } else if h >= 1400 { "QHD" } else if h >= 1000 || w >= 1800 { "FHD" } else if h >= 700 { "HD" } else { "SD" };
    format!("{q} · {w}×{h}")
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        self.poll_player();
        self.update_awake();
        self.track_window(ctx);
        if self.slots.iter().any(|s| s.current.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let typing = ctx.egui_wants_keyboard_input();
        if !typing && !self.show_settings {
            if self.multi {
                for (n, k) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4].into_iter().enumerate() {
                    if ctx.input(|i| i.key_pressed(k)) { self.set_active(n); }
                }
            } else {
                let digits: String = ctx.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.as_str()) } else { None }).collect::<String>());
                let digits: String = digits.chars().filter(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() && self.chan_buf.len() < 5 {
                    self.chan_buf.push_str(&digits);
                    self.chan_at = std::time::Instant::now();
                }
            }
            let (space, enter, f, m, l, r, u, d, slash, esc, f11) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::Space),
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::F),
                    i.key_pressed(egui::Key::M),
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::Slash),
                    i.key_pressed(egui::Key::Escape),
                    i.key_pressed(egui::Key::F11),
                )
            });
            if !self.chan_buf.is_empty() {
                if esc {
                    self.chan_buf.clear();
                } else if enter || self.chan_at.elapsed().as_millis() > 1800 {
                    let n = self.chan_buf.parse().unwrap_or(0);
                    self.chan_buf.clear();
                    self.play_number(n);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
            }
            let vod = self.is_vod();
            if space { self.toggle_pause(); }
            if f || f11 { self.toggle_fullscreen(ctx); }
            if esc && self.fullscreen { self.toggle_fullscreen(ctx); }
            else if esc && self.pip { self.toggle_pip(ctx); }
            let (p_key, r_key, s_key, pgu, pgd) = ctx.input(|i| (i.key_pressed(egui::Key::P), i.key_pressed(egui::Key::R), i.key_pressed(egui::Key::S), i.key_pressed(egui::Key::PageUp), i.key_pressed(egui::Key::PageDown)));
            if p_key { self.toggle_pip(ctx); }
            if r_key { self.retry_now(); }
            if m { self.toggle_mute(); }
            if s_key {
                if let Some(c) = self.cur().current.as_ref().map(|c| c.item.clone()) { self.toggle_fav(&c); }
            }
            if vod { if l { self.seek_rel(-10.0) } if r { self.seek_rel(10.0) } }
            if u { self.set_volume(self.volume + 5.0) }
            if d { self.set_volume(self.volume - 5.0) }
            if pgu { self.step(-1); }
            if pgd { self.step(1); }
            if slash { ctx.memory_mut(|m| m.request_focus(egui::Id::unique("search"))); }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        set_keep_awake(false);
        self.save_settings();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.view_dirty { self.recompute_view(); }
        if !self.compact() {
            egui::Panel::left("side").exact_size(380.0).resizable(false).frame(egui::Frame::new().fill(PANEL).inner_margin(12.0)).show(ui, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| self.main_area(ui, frame));
        if self.show_settings { self.settings_window(&ctx); }
    }
}

impl App {
    /// Remember the normal (not maximized / fullscreen) window geometry for the next launch.
    fn track_window(&mut self, ctx: &egui::Context) {
        let (outer, inner, maximized, full) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.inner_rect, v.maximized.unwrap_or(false), v.fullscreen.unwrap_or(false))
        });
        if full || self.pip {
            return;
        }
        self.settings.win_maximized = maximized;
        if let (Some(o), Some(i), false) = (outer, inner, maximized) {
            if o.min.x > -10000.0 && i.width() > 100.0 {
                self.settings.win = Some([o.min.x, o.min.y, i.width(), i.height()]);
            }
        }
    }

    fn chan_overlay(&self, ui: &egui::Ui, area: egui::Rect) {
        if self.chan_buf.is_empty() {
            return;
        }
        let g = ui.painter().layout_no_wrap(self.chan_buf.clone(), egui::FontId::proportional(40.0), Color32::WHITE);
        let r = egui::Rect::from_min_size(egui::pos2(area.max.x - g.size().x - 40.0, area.min.y + 12.0), g.size() + Vec2::new(24.0, 12.0));
        ui.painter().rect_filled(r, 10.0, Color32::from_black_alpha(190));
        ui.painter().galley(r.min + Vec2::new(12.0, 6.0), g, Color32::WHITE);
    }

    fn compact(&self) -> bool {
        self.fullscreen || self.pip
    }

    fn toggle_pip(&mut self, ctx: &egui::Context) {
        if self.fullscreen {
            self.toggle_fullscreen(ctx);
        }
        self.pip = !self.pip;
        use egui::viewport::WindowLevel;
        use egui::ViewportCommand as C;
        if self.pip {
            ctx.send_viewport_cmd(C::MinInnerSize(Vec2::new(320.0, 200.0)));
            ctx.send_viewport_cmd(C::InnerSize(Vec2::new(480.0, 300.0)));
            ctx.send_viewport_cmd(C::WindowLevel(WindowLevel::AlwaysOnTop));
        } else {
            ctx.send_viewport_cmd(C::WindowLevel(WindowLevel::Normal));
            ctx.send_viewport_cmd(C::MinInnerSize(Vec2::new(900.0, 560.0)));
            let [w, h] = self.settings.win.map(|w| [w[2], w[3]]).unwrap_or([1360.0, 820.0]);
            ctx.send_viewport_cmd(C::InnerSize(Vec2::new(w, h)));
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        if self.pip {
            self.toggle_pip(ctx);
        }
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                icons::paint(ui.painter(), Icon::Tv, r, ACCENT);
                ui.label(RichText::new("IPTV").size(20.0).strong().color(ACCENT));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, Icon::Gear, 16.0, false, "Source settings").clicked() { self.show_settings = true; }
                if icons::button(ui, Icon::Refresh, 16.0, false, "Refresh list").clicked() { let k = self.kind; self.refresh(k); }
                if icons::button(ui, Icon::Grid, 16.0, self.multi, "Multi-view: watch up to 4 streams at once (keys 1-4 select a tile)").clicked() { let m = !self.multi; self.set_multi(m); }
            });
        });
        if self.multi {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for n in 2..=MAX_SLOTS {
                    if ui.selectable_label(self.multi_n == n, format!("{n} screens")).clicked() { self.set_multi_count(n); }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for l in MultiLayout::ALL {
                    if ui.selectable_label(self.layout == l, l.label()).clicked() { self.layout = l; }
                }
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for k in Kind::ALL {
                if ui.selectable_label(self.kind == k, RichText::new(k.label()).size(14.0)).clicked() { self.open_kind(k); }
            }
        });
        let te = egui::TextEdit::singleline(&mut self.query).id(egui::Id::unique("search")).hint_text("Search  ( / )").desired_width(f32::INFINITY);
        if ui.add(te).changed() { self.view_dirty = true; }

        let ki = self.kind.idx();
        let cur_label = match &self.group { Group::All => "All".to_string(), Group::Fav => "Favorites".into(), Group::Recent => "Recent".into(), Group::Named(n) => n.clone() };
        let show_cats = self.browsing && self.query.trim().is_empty() && self.series.is_none();
        if show_cats {
            ui.add_space(2.0);
            if self.loading[ki] && self.groups[ki].is_empty() {
                ui.horizontal(|ui| { ui.spinner(); ui.label("Loading…"); });
            } else if let Some(e) = self.lib_err[ki].clone() {
                ui.colored_label(Color32::from_rgb(240, 110, 110), e);
            }
            let mut pick: Option<Group> = None;
            let total = self.libs[ki].len();
            ui.label(RichText::new("Categories").small().color(MUTED));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut row = |ui: &mut egui::Ui, g: Group, label: String, icon: Option<Icon>| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), egui::Sense::click());
                    let bg = if resp.hovered() { Color32::from_rgb(34, 38, 54) } else { Color32::from_rgb(26, 30, 42) };
                    ui.painter().rect_filled(rect, 6.0, bg);
                    let mut x = rect.min.x + 12.0;
                    if let Some(ic) = icon {
                        let col = if ic == Icon::Star { Color32::from_rgb(250, 204, 21) } else { MUTED };
                        icons::paint(ui.painter(), ic, egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), col);
                        x += 26.0;
                    }
                    let g_text = ui.painter().layout(label, egui::FontId::proportional(14.0), ui.visuals().text_color(), rect.max.x - x - 8.0);
                    ui.painter().galley(egui::pos2(x, rect.center().y - g_text.size().y / 2.0), g_text, ui.visuals().text_color());
                    if resp.clicked() { pick = Some(g); }
                };
                row(ui, Group::All, format!("All  ({total})"), None);
                row(ui, Group::Fav, "Favorites".into(), Some(Icon::Star));
                row(ui, Group::Recent, "Recent".into(), None);
                for (g, n) in self.groups[ki].iter().filter(|(g, _)| !(self.locked && is_adult(g))) {
                    row(ui, Group::Named(g.clone()), format!("{g}  ({n})"), None);
                }
            });
            if let Some(g) = pick { self.group = g; self.browsing = false; self.view_dirty = true; }
            return;
        }
        if self.series.is_none() && icons::text_button(ui, Icon::Back, &format!("Categories  ·  {cur_label}"), false).clicked() { self.browsing = true; return; }
        ui.add_space(2.0);
        if self.loading[ki] && self.view.is_empty() {
            ui.horizontal(|ui| { ui.spinner(); ui.label("Loading…"); });
        } else if let Some(e) = self.lib_err[ki].clone() {
            ui.colored_label(Color32::from_rgb(240, 110, 110), e);
        } else {
            ui.label(RichText::new(format!("{} items", self.view.len())).small().color(MUTED));
        }
        ui.add_space(2.0);

        if let Some(sv) = &self.series {
            let _ = sv;
            self.series_list(ui);
            return;
        }
        self.channel_list(ui);
    }

    fn channel_list(&mut self, ui: &mut egui::Ui) {
        let row_h = 52.0;
        let lib = self.libs[self.kind.idx()].clone();
        let now = now_ms();
        let cur_urls: Vec<String> = self.slots.iter().filter_map(|s| s.current.as_ref().map(|c| c.item.url.clone())).collect();
        let mut open: Option<Item> = None;
        let mut fav: Option<Item> = None;
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(i) = self.scroll_to.take() {
            area = area.vertical_scroll_offset((i as f32 * (row_h + ui.spacing().item_spacing.y) - 150.0).max(0.0));
        }
        let view = std::mem::take(&mut self.view);
        area.show_rows(ui, row_h, view.len(), |ui, range| {
            for r in range {
                let Some(it) = view.get(r).and_then(|&i| lib.get(i as usize)) else { continue };
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), egui::Sense::click());
                let playing = cur_urls.iter().any(|u| u == &it.url);
                let bg = if playing { Color32::from_rgb(38, 40, 80) } else if resp.hovered() { Color32::from_rgb(30, 34, 46) } else { Color32::TRANSPARENT };
                ui.painter().rect_filled(rect, 8.0, bg);
                let logo_rect = egui::Rect::from_min_size(rect.min + Vec2::new(6.0, 6.0), Vec2::splat(40.0));
                if !it.logo.is_empty() && it.logo.starts_with("http") {
                    egui::Image::new(it.logo.as_str()).fit_to_exact_size(Vec2::splat(40.0)).corner_radius(6.0).paint_at(ui, logo_rect);
                } else {
                    ui.painter().rect_filled(logo_rect, 6.0, Color32::from_rgb(34, 38, 50));
                }
                let tx = rect.min.x + 54.0;
                ui.painter().text(egui::pos2(tx, rect.min.y + 8.0), egui::Align2::LEFT_TOP, &it.name, egui::FontId::proportional(14.0), if playing { Color32::WHITE } else { Color32::from_rgb(220, 224, 235) });
                let sub = if self.kind == Kind::Live {
                    self.now_playing(it, now).map(|p| p.0).unwrap_or_else(|| it.group.clone())
                } else if !it.rating.is_empty() {
                    format!("Rating {}  ·  {}", it.rating, it.group)
                } else { it.group.clone() };
                let clipped = ui.painter().layout(sub, egui::FontId::proportional(12.0), MUTED, rect.width() - 54.0 - 36.0);
                ui.painter().galley(egui::pos2(tx, rect.min.y + 29.0), clipped, MUTED);
                let is_fav = self.favs.contains(&it.key());
                let star = egui::Rect::from_center_size(egui::pos2(rect.max.x - 18.0, rect.center().y), Vec2::splat(24.0));
                let sr = ui.interact(star, ui.scope_id().with(("fav", r)), egui::Sense::click());
                paint_star_icon(ui.painter(), star, is_fav);
                if sr.clicked() { fav = Some(it.clone()); }
                else if resp.clicked() { open = Some(it.clone()); }
            }
        });
        self.view = view;
        if let Some(it) = fav { self.toggle_fav(&it); }
        if let Some(it) = open { self.open_item(&it); }
    }

    fn series_list(&mut self, ui: &mut egui::Ui) {
        let mut back = false;
        let mut play: Option<usize> = None;
        let Some(sv) = &self.series else { return };
        ui.horizontal(|ui| {
            if icons::text_button(ui, Icon::Back, "Back", false).clicked() { back = true; }
            ui.label(RichText::new(&sv.item.name).strong());
        });
        ui.separator();
        match (&sv.info, &sv.err) {
            (_, Some(e)) => { ui.colored_label(Color32::from_rgb(240, 110, 110), e); }
            (None, _) => { ui.horizontal(|ui| { ui.spinner(); ui.label("Loading episodes…"); }); }
            (Some(info), _) => {
                let cur = self.cur().current.as_ref().filter(|c| c.episodes.is_some() && c.item.series_id == sv.item.series_id).map(|c| c.ep_idx);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut last = String::new();
                    for (i, e) in info.episodes.iter().enumerate() {
                        if e.season != last { last = e.season.clone(); ui.add_space(6.0); ui.label(RichText::new(format!("Season {}", e.season)).strong().color(ACCENT)); }
                        let label = format!("{:>2}.  {}", e.num, e.title);
                        if ui.add_sized([ui.available_width(), 28.0], egui::Button::selectable(cur == Some(i), label)).clicked() { play = Some(i); }
                    }
                });
            }
        }
        if back { self.series = None; }
        if let Some(i) = play { self.play_episode(i); }
    }

    fn main_area(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let avail = ui.available_rect_before_wrap();
        let info_h = if self.compact() { 0.0 } else { 150.0f32.min(avail.height() * 0.3) };
        let video_rect = egui::Rect::from_min_size(avail.min, Vec2::new(avail.width(), avail.height() - info_h));
        let ppp = ui.ctx().pixels_per_point();
        let n = if self.multi { self.slots.len() } else { 1 };
        let tiles: Vec<egui::Rect> = if n == 1 { vec![video_rect] } else { tile_rects(video_rect, n, self.layout, self.active) };
        let mut close: Option<usize> = None;
        for (i, &tr) in tiles.iter().enumerate() {
            ui.painter().rect_filled(tr, 0.0, Color32::BLACK);
            let has = self.slots[i].current.is_some();
            if has {
                if let Some(id) = self.render_video(i, frame, (tr.width() * ppp) as i32, (tr.height() * ppp) as i32) {
                    ui.painter().image(id, tr, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                }
            } else {
                let msg = if let Some(e) = &self.mpv_err { format!("Player unavailable: {e}") } else if n > 1 { format!("Tile {} — pick a channel", i + 1) } else { "Select a channel to start watching".into() };
                ui.painter().text(tr.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(if n > 1 { 14.0 } else { 18.0 }), MUTED);
            }

            let resp = ui.interact(tr, ui.scope_id().with(("video", i)), egui::Sense::click());
            if resp.double_clicked() { let c = ui.ctx().clone(); self.toggle_fullscreen(&c); }
            else if resp.clicked() {
                if i == self.active { self.toggle_pause(); } else { self.set_active(i); }
            }
            if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) { self.last_mouse_move = std::time::Instant::now(); }

            if has {
                let s = &self.slots[i];
                let status = if s.stats.buffering && s.status.is_empty() { "Buffering…".to_string() } else { s.status.clone() };
                if !status.is_empty() {
                    ui.painter().text(tr.center(), egui::Align2::CENTER_CENTER, status, egui::FontId::proportional(if n > 1 { 14.0 } else { 18.0 }), Color32::WHITE);
                }
            }
            if n > 1 {
                if let Some(c) = &self.slots[i].current {
                    let g = ui.painter().layout(format!("{} · {}", i + 1, c.title), egui::FontId::proportional(12.0), Color32::WHITE, tr.width() - 80.0);
                    let r = egui::Rect::from_min_size(egui::pos2(tr.min.x + 6.0, tr.max.y - g.size().y - 14.0), g.size() + Vec2::new(12.0, 8.0));
                    ui.painter().rect_filled(r, 6.0, Color32::from_black_alpha(170));
                    ui.painter().galley(r.min + Vec2::new(6.0, 4.0), g, Color32::WHITE);
                    let xr = egui::Rect::from_min_size(egui::pos2(tr.max.x - 30.0, tr.min.y + 6.0), Vec2::splat(24.0));
                    let xresp = ui.interact(xr, ui.scope_id().with(("close", i)), egui::Sense::click());
                    ui.painter().rect_filled(xr, 12.0, Color32::from_black_alpha(if xresp.hovered() { 220 } else { 150 }));
                    icons::paint(ui.painter(), Icon::Close, xr.shrink(6.0), Color32::WHITE);
                    if xresp.on_hover_text("Close stream").clicked() { close = Some(i); }
                }
                let col = if i == self.active { ACCENT } else { Color32::from_rgb(34, 38, 50) };
                ui.painter().rect_stroke(tr, 0.0, egui::Stroke::new(if i == self.active { 2.0 } else { 1.0 }, col), egui::StrokeKind::Inside);
            }
        }
        if let Some(i) = close { self.stop_slot(i); }
        self.chan_overlay(ui, video_rect);

        let any = self.slots.iter().any(|s| s.current.is_some());
        let show_ctl = self.last_mouse_move.elapsed().as_secs_f32() < 3.0 || self.cur().stats.paused || !self.fullscreen;
        if any && !show_ctl { ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::None); }

        if self.cur().current.is_some() && show_ctl { self.controls(ui, video_rect); }

        if !self.compact() {
            let info = egui::Rect::from_min_max(egui::pos2(avail.min.x, video_rect.max.y), avail.max);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(info.shrink(14.0)));
            self.info_panel(&mut child);
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui, vr: egui::Rect) {
        let h = 74.0;
        let bar = egui::Rect::from_min_max(egui::pos2(vr.min.x, vr.max.y - h), vr.max);
        ui.painter().rect_filled(bar, 0.0, Color32::from_black_alpha(150));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(Vec2::new(14.0, 8.0))));
        let ui = &mut child;
        let vod = self.is_vod() && self.cur().stats.dur > 0.5;
        if vod {
            ui.horizontal(|ui| {
                let (pos, dur) = (self.cur().stats.pos, self.cur().stats.dur);
                ui.label(RichText::new(fmt_time(pos)).monospace());
                let mut p = pos;
                ui.spacing_mut().slider_width = (ui.available_width() - 90.0).max(50.0);
                let s = egui::Slider::new(&mut p, 0.0..=dur).show_value(false);
                let r = ui.add(s);
                if r.drag_stopped() || (r.changed() && !r.dragged()) { self.seek_abs(p); }
                ui.label(RichText::new(fmt_time(dur)).monospace());
            });
        }
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Prev, 18.0, false, "Previous").clicked() { self.step(-1); }
            let (pi, ptip) = if self.cur().stats.paused { (Icon::Play, "Play (Space)") } else { (Icon::Pause, "Pause (Space)") };
            if icons::button(ui, pi, 18.0, false, ptip).clicked() { self.toggle_pause(); }
            if icons::button(ui, Icon::Next, 18.0, false, "Next").clicked() { self.step(1); }
            if icons::button(ui, Icon::Stop, 18.0, false, "Stop").clicked() { self.stop(); }
            ui.separator();
            if icons::button(ui, if self.muted { Icon::Mute } else { Icon::Volume }, 18.0, false, "Mute (M)").clicked() { self.toggle_mute(); }
            let mut v = self.volume;
            if ui.add(egui::Slider::new(&mut v, 0.0..=130.0).show_value(false)).changed() { self.set_volume(v); }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let c = ui.ctx().clone();
                if icons::button(ui, Icon::Fullscreen, 18.0, false, "Fullscreen (F / F11)").clicked() { self.toggle_fullscreen(&c); }
                if icons::button(ui, Icon::Pip, 18.0, self.pip, "Small always-on-top window (P)").clicked() { self.toggle_pip(&c); }
                let gear = icons::button(ui, Icon::Gear, 18.0, false, "Audio, subtitles, aspect ratio, speed, hardware decoding");
                egui::Popup::menu(&gear).show(|ui| self.player_menu(ui));
                if let Some(it) = self.cur().current.as_ref().map(|c| c.item.clone()) {
                    let fav = self.favs.contains(&it.key());
                    if icons::text_button(ui, if fav { Icon::Star } else { Icon::StarOutline }, if fav { "Unfavorite" } else { "Favorite" }, false).clicked() { self.toggle_fav(&it); }
                }
                if !vod {
                    ui.label(RichText::new("LIVE").color(Color32::from_rgb(239, 68, 68)).strong());
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
                    icons::paint(ui.painter(), Icon::Dot, r, Color32::from_rgb(239, 68, 68));
                }
            });
        });
    }

    fn player_menu(&mut self, ui: &mut egui::Ui) {
        let tracks = self.cur().tracks.clone();
        let audio: Vec<_> = tracks.iter().filter(|t| t.audio).collect();
        let subs: Vec<_> = tracks.iter().filter(|t| !t.audio).collect();
        ui.menu_button("Audio track", |ui| {
            if audio.is_empty() { ui.label("No audio tracks"); }
            for t in &audio {
                if ui.selectable_label(t.selected, t.label()).clicked() { self.set_audio_track(t.id); ui.close(); }
            }
        });
        ui.menu_button("Subtitles", |ui| {
            if ui.selectable_label(!subs.iter().any(|t| t.selected), "Off").clicked() { self.set_sub_track(-1); ui.close(); }
            for t in &subs {
                if ui.selectable_label(t.selected, t.label()).clicked() { self.set_sub_track(t.id); ui.close(); }
            }
        });
        ui.menu_button("Aspect ratio", |ui| {
            for (i, (label, _, _)) in ASPECTS.iter().enumerate() {
                if ui.selectable_label(self.aspect == i, *label).clicked() { self.set_aspect(i); ui.close(); }
            }
        });
        if self.is_vod() {
            ui.menu_button("Speed", |ui| {
                for v in SPEEDS {
                    if ui.selectable_label(self.speed == v, format!("{v}×")).clicked() { self.set_speed(v); ui.close(); }
                }
            });
        }
        ui.separator();
        let mut hw = self.hwdec;
        if ui.checkbox(&mut hw, "Hardware decoding").changed() { self.set_hwdec(hw); }
        if self.cur().current.as_ref().is_some_and(|c| c.kind == Kind::Live) && ui.button("Reconnect (R)").clicked() { self.retry_now(); ui.close(); }
    }

    fn info_panel(&mut self, ui: &mut egui::Ui) {
        let Some(cur) = &self.slots[self.active].current else {
            ui.label(RichText::new("Shortcuts:  / search · Up/Down volume · Left/Right seek (movies) · PgUp/PgDn channel · Space pause · F/F11 fullscreen · S favorite · P picture-in-picture · R reconnect · M mute · Multi-view: 1-4 select tile · digits: channel number").color(MUTED));
            return;
        };
        let st = &self.slots[self.active].stats;
        ui.horizontal(|ui| {
            ui.label(RichText::new(&cur.title).size(18.0).strong());
            if st.w > 0 {
                ui.label(RichText::new(res_label(st.w, st.h)).color(ACCENT).strong());
            }
        });
        if cur.kind == Kind::Live {
            let item = cur.item.clone();
            if let Some(progs) = self.programs_for(&item) {
                let now = now_ms();
                let upcoming: Vec<&Prog> = progs.iter().filter(|p| p.stop > now).take(4).collect();
                if upcoming.is_empty() { ui.label(RichText::new("No guide data").color(MUTED)); }
                for (i, p) in upcoming.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}–{}", fmt_clock(p.start), fmt_clock(p.stop))).monospace().color(MUTED));
                        ui.label(if i == 0 && p.start <= now { RichText::new(&p.title).strong().color(ACCENT) } else { RichText::new(&p.title) });
                    });
                    if i == 0 && p.start <= now && p.stop > p.start {
                        ui.add(egui::ProgressBar::new(((now - p.start) as f32 / (p.stop - p.start) as f32).clamp(0.0, 1.0)).desired_height(4.0));
                    }
                }
            } else {
                ui.label(RichText::new("No guide data").color(MUTED));
            }
        } else if !cur.item.plot.is_empty() {
            ui.label(RichText::new(&cur.item.plot).color(MUTED));
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Source").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).fixed_size([420.0, 0.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.form_xtream, true, "Xtream login");
                ui.selectable_value(&mut self.form_xtream, false, "M3U playlist");
            });
            ui.add_space(6.0);
            if !self.settings.saved.is_empty() {
                ui.label(RichText::new("Saved sources").strong());
                let mut use_idx = None;
                let mut del_idx = None;
                for (i, s) in self.settings.saved.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let label = if s.xtream { format!("{}@{}", s.user, s.server) } else { s.playlist.clone() };
                        let active = *s == self.src;
                        if ui.add_enabled(!self.form_busy, egui::Button::selectable(active, label)).clicked() { use_idx = Some(i); }
                        if crate::icons::button(ui, Icon::Trash, 13.0, false, "Delete").clicked() { del_idx = Some(i); }
                    });
                }
                if let Some(i) = del_idx { self.settings.saved.remove(i); self.save_settings(); }
                if let Some(i) = use_idx {
                    let s = self.settings.saved[i].clone();
                    self.form_xtream = s.xtream;
                    self.form = s;
                    self.connect();
                }
                ui.separator();
                ui.label(RichText::new("Add / edit").strong());
            }
            let w = f32::INFINITY;
            if self.form_xtream {
                ui.label("Server URL");
                ui.add(egui::TextEdit::singleline(&mut self.form.server).hint_text("http://host:port").desired_width(w));
                ui.label("Username");
                ui.add(egui::TextEdit::singleline(&mut self.form.user).desired_width(w));
                ui.label("Password");
                ui.add(egui::TextEdit::singleline(&mut self.form.pass).password(true).desired_width(w));
            } else {
                ui.label("Playlist URL or file path");
                ui.add(egui::TextEdit::singleline(&mut self.form.playlist).desired_width(w));
                ui.label("EPG (XMLTV) URL — optional");
                ui.add(egui::TextEdit::singleline(&mut self.form.epg).desired_width(w));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.form_busy, egui::Button::new("Connect").fill(ACCENT)).clicked() { self.connect(); }
                if self.form_busy { ui.spinner(); }
                ui.label(RichText::new(&self.form_msg).color(MUTED));
            });
            ui.separator();
            ui.label(RichText::new("Parental control").strong());
            let has_pin = !self.settings.pin.is_empty();
            if has_pin && self.locked {
                ui.label(RichText::new("Adult categories are hidden. Enter the PIN to show them.").color(MUTED));
                ui.horizontal(|ui| {
                    let te = ui.add(egui::TextEdit::singleline(&mut self.pin_input).password(true).hint_text("PIN").desired_width(120.0));
                    let go = ui.button("Unlock").clicked() || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go { self.try_unlock(); }
                });
            } else if has_pin {
                ui.label(RichText::new("Unlocked for this session.").color(MUTED));
                ui.horizontal(|ui| {
                    if ui.button("Lock now").clicked() { self.lock_now(); }
                    if ui.button("Remove PIN").clicked() { self.remove_pin(); }
                });
            } else {
                ui.label(RichText::new("Set a PIN to hide adult categories at every launch.").color(MUTED));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.pin_input).password(true).hint_text("New PIN (4-8 digits)").desired_width(160.0));
                    if ui.button("Set PIN").clicked() { self.set_pin(); }
                });
            }
            if !self.pin_msg.is_empty() { ui.label(RichText::new(&self.pin_msg).color(MUTED)); }
        });
        if !open { self.show_settings = false; }
    }
}
