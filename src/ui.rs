use crate::app::*;
use crate::data::*;
use egui::{Color32, RichText, Vec2};

const BG: Color32 = Color32::from_rgb(14, 16, 22);
const PANEL: Color32 = Color32::from_rgb(20, 23, 31);
const ACCENT: Color32 = Color32::from_rgb(99, 102, 241);
const MUTED: Color32 = Color32::from_rgb(140, 148, 166);

pub fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(24, 28, 38);
    v.extreme_bg_color = BG;
    v.selection.bg_fill = ACCENT;
    v.widgets.noninteractive.bg_stroke.color = Color32::from_rgb(34, 38, 50);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
    });
}

fn paint_star(p: &egui::Painter, c: egui::Pos2, r: f32, filled: bool, col: Color32) {
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            egui::pos2(c.x + rr * a.cos(), c.y + rr * a.sin())
        })
        .collect();
    if filled {
        p.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    } else {
        p.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.5, col)));
    }
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

fn res_label(w: i64, h: i64) -> String {
    let q = if h >= 2000 || w >= 3600 { "4K" } else if h >= 1400 { "QHD" } else if h >= 1000 || w >= 1800 { "FHD" } else if h >= 700 { "HD" } else { "SD" };
    format!("{q} · {w}×{h}")
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        self.poll_player();
        if self.current.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let typing = ctx.egui_wants_keyboard_input();
        if !typing && !self.show_settings {
            let (space, enter, f, m, l, r, u, d, slash, esc) = ctx.input(|i| {
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
                )
            });
            let vod = self.is_vod();
            if space { self.toggle_pause(); }
            if enter { self.toggle_fullscreen(ctx); }
            if esc && self.fullscreen { self.toggle_fullscreen(ctx); }
            if m { self.toggle_mute(); }
            if f {
                if let Some(c) = self.current.as_ref().map(|c| c.item.clone()) { self.toggle_fav(&c); }
            }
            if l { if vod { self.seek_rel(-10.0) } else { self.set_volume(self.volume - 5.0) } }
            if r { if vod { self.seek_rel(10.0) } else { self.set_volume(self.volume + 5.0) } }
            if u { self.step(-1); }
            if d { self.step(1); }
            if slash { ctx.memory_mut(|m| m.request_focus(egui::Id::new("search"))); }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.view_dirty { self.recompute_view(); }
        if !self.fullscreen {
            egui::Panel::left("side").exact_size(380.0).resizable(false).frame(egui::Frame::new().fill(PANEL).inner_margin(12.0)).show_inside(ui, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show_inside(ui, |ui| self.main_area(ui, frame));
        if self.show_settings { self.settings_window(&ctx); }
    }
}

impl App {
    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("◉ IPTV").size(20.0).strong().color(ACCENT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⚙").on_hover_text("Source settings").clicked() { self.show_settings = true; }
                if ui.button("⟳").on_hover_text("Refresh list").clicked() { let k = self.kind; self.refresh(k); }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for k in Kind::ALL {
                if ui.selectable_label(self.kind == k, RichText::new(k.label()).size(14.0)).clicked() { self.open_kind(k); }
            }
        });
        let te = egui::TextEdit::singleline(&mut self.query).id(egui::Id::new("search")).hint_text("Search  ( / )").desired_width(f32::INFINITY);
        if ui.add(te).changed() { self.view_dirty = true; }

        let ki = self.kind.idx();
        let cur_label = match &self.group { Group::All => "All".to_string(), Group::Fav => "Favorites".into(), Group::Recent => "Recent".into(), Group::Named(n) => n.clone() };
        let mut new_group = None;
        egui::ComboBox::from_id_salt("grp").width(ui.available_width()).selected_text(cur_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut new_group, Some(Group::All), "All");
            ui.selectable_value(&mut new_group,             Some(Group::Fav), "Favorites");
                        ui.selectable_value(&mut new_group, Some(Group::Recent), "Recent");
            ui.separator();
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for (g, n) in &self.groups[ki] {
                    ui.selectable_value(&mut new_group, Some(Group::Named(g.clone())), format!("{g}  ({n})"));
                }
            });
        });
        if let Some(g) = new_group { if g != self.group { self.group = g; self.view_dirty = true; } }

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
        let cur_url = self.current.as_ref().map(|c| c.item.url.clone());
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
                let playing = cur_url.as_deref() == Some(it.url.as_str());
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
                let sr = ui.interact(star, ui.id().with(("fav", r)), egui::Sense::click());
                paint_star(ui.painter(), star.center(), 9.0, is_fav, if is_fav { Color32::from_rgb(250, 204, 21) } else { MUTED });
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
            if ui.button("← Back").clicked() { back = true; }
            ui.label(RichText::new(&sv.item.name).strong());
        });
        ui.separator();
        match (&sv.info, &sv.err) {
            (_, Some(e)) => { ui.colored_label(Color32::from_rgb(240, 110, 110), e); }
            (None, _) => { ui.horizontal(|ui| { ui.spinner(); ui.label("Loading episodes…"); }); }
            (Some(info), _) => {
                let cur = self.current.as_ref().filter(|c| c.episodes.is_some() && c.item.series_id == sv.item.series_id).map(|c| c.ep_idx);
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
        let info_h = if self.fullscreen { 0.0 } else { 150.0f32.min(avail.height() * 0.3) };
        let video_rect = egui::Rect::from_min_size(avail.min, Vec2::new(avail.width(), avail.height() - info_h));
        let ppp = ui.ctx().pixels_per_point();
        ui.painter().rect_filled(video_rect, 0.0, Color32::BLACK);

        if self.current.is_some() {
            let w = (video_rect.width() * ppp) as i32;
            let h = (video_rect.height() * ppp) as i32;
            if let Some(id) = self.render_video(frame, w, h) {
                ui.painter().image(id, video_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
        } else {
            let msg = if let Some(e) = &self.mpv_err { format!("Player unavailable: {e}") } else { "Select a channel to start watching".into() };
            ui.painter().text(video_rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(18.0), MUTED);
        }

        let resp = ui.interact(video_rect, ui.id().with("video"), egui::Sense::click());
        if resp.double_clicked() { let c = ui.ctx().clone(); self.toggle_fullscreen(&c); }
        else if resp.clicked() { self.toggle_pause(); }
        if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) { self.last_mouse_move = std::time::Instant::now(); }
        let show_ctl = self.last_mouse_move.elapsed().as_secs_f32() < 3.0 || self.stats.paused || !self.fullscreen;
        if self.current.is_some() && !show_ctl { ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::None); }

        if self.current.is_some() {
            if self.stats.w > 0 {
                let t = res_label(self.stats.w, self.stats.h);
                let g = ui.painter().layout_no_wrap(t, egui::FontId::proportional(12.0), Color32::WHITE);
                let r = egui::Rect::from_min_size(video_rect.min + Vec2::new(12.0, 12.0), g.size() + Vec2::new(16.0, 8.0));
                ui.painter().rect_filled(r, 12.0, Color32::from_black_alpha(170));
                ui.painter().galley(r.min + Vec2::new(8.0, 4.0), g, Color32::WHITE);
            }
            let status = if self.stats.buffering && self.status.is_empty() { "Buffering…".to_string() } else { self.status.clone() };
            if !status.is_empty() {
                ui.painter().text(video_rect.center(), egui::Align2::CENTER_CENTER, status, egui::FontId::proportional(18.0), Color32::WHITE);
            }
            if show_ctl { self.controls(ui, video_rect); }
        }

        if !self.fullscreen {
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
        let vod = self.is_vod() && self.stats.dur > 0.5;
        if vod {
            ui.horizontal(|ui| {
                ui.label(RichText::new(fmt_time(self.stats.pos)).monospace());
                let mut p = self.stats.pos;
                ui.spacing_mut().slider_width = (ui.available_width() - 90.0).max(50.0);
                let s = egui::Slider::new(&mut p, 0.0..=self.stats.dur).show_value(false);
                let r = ui.add(s);
                if r.drag_stopped() || (r.changed() && !r.dragged()) { self.seek_abs(p); }
                ui.label(RichText::new(fmt_time(self.stats.dur)).monospace());
            });
        }
        ui.horizontal(|ui| {
            let big = |s: &str| RichText::new(s).size(18.0);
            if ui.button(big("⏮")).clicked() { self.step(-1); }
            if ui.button(big(if self.stats.paused { "▶" } else { "⏸" })).clicked() { self.toggle_pause(); }
            if ui.button(big("⏭")).clicked() { self.step(1); }
            if ui.button(big("⏹")).clicked() { self.stop(); }
            ui.separator();
            if ui.button(big(if self.muted { "🔇" } else { "🔊" })).clicked() { self.toggle_mute(); }
            let mut v = self.volume;
            if ui.add(egui::Slider::new(&mut v, 0.0..=130.0).show_value(false)).changed() { self.set_volume(v); }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let c = ui.ctx().clone();
                if ui.button(RichText::new("Fullscreen").size(14.0)).on_hover_text("Fullscreen (Enter)").clicked() { self.toggle_fullscreen(&c); }
                if let Some(it) = self.current.as_ref().map(|c| c.item.clone()) {
                    let fav = self.favs.contains(&it.key());
                    if ui.button(big(if fav { "Unfavorite" } else { "Favorite" }).size(14.0)).clicked() { self.toggle_fav(&it); }
                }
                if !vod { ui.label(RichText::new("● LIVE").color(Color32::from_rgb(239, 68, 68)).strong()); }
            });
        });
    }

    fn info_panel(&mut self, ui: &mut egui::Ui) {
        let Some(cur) = &self.current else {
            ui.label(RichText::new("Shortcuts:  / search · ↑↓ channel · Space pause · Enter fullscreen · F favorite · M mute · ←→ seek/volume").color(MUTED));
            return;
        };
        ui.label(RichText::new(&cur.title).size(18.0).strong());
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
        });
        if !open { self.show_settings = false; }
    }
}
