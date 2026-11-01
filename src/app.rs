//! Application state and logic (no drawing).
use crate::data::*;
use crate::mpv::{Event_, Mpv};
use eframe::glow;
use eframe::glow::HasContext;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum Msg {
    Connected { src: Source },
    ConnectFailed(String),
    Library { kind: Kind, items: Vec<Item>, fresh: bool },
    LibraryFailed { kind: Kind, err: String },
    Epg(Epg),
    ShortEpg { sid: u64, progs: Vec<Prog> },
    Series { id: u64, info: SeriesInfo },
    SeriesFailed { id: u64, err: String },
    Note(String),
}

#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub source: Source,
    pub favs: Vec<String>,
    pub recent: Vec<String>,
    pub volume: Option<f64>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Group {
    All,
    Fav,
    Recent,
    Named(String),
}

pub struct Playing {
    pub item: Item,
    pub kind: Kind,
    pub episodes: Option<Arc<Vec<Episode>>>,
    pub ep_idx: usize,
    pub title: String,
    pub retried: bool,
}

pub struct SeriesView {
    pub item: Item,
    pub info: Option<Arc<SeriesInfo>>,
    pub err: Option<String>,
}

#[derive(Default, Clone)]
pub struct Stats {
    pub pos: f64,
    pub dur: f64,
    pub paused: bool,
    pub buffering: bool,
    pub w: i64,
    pub h: i64,
}

pub struct VideoTex {
    pub fbo: glow::Framebuffer,
    pub tex: glow::Texture,
    pub id: egui::TextureId,
    pub w: i32,
    pub h: i32,
}

pub struct App {
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    pub ctx: egui::Context,
    pub mpv: Option<Mpv>,
    pub mpv_err: Option<String>,
    pub video: Option<VideoTex>,
    pub force_render: bool,

    pub settings: Settings,
    pub src: Source,
    pub favs: HashSet<String>,
    pub recent: Vec<String>,

    pub libs: [Arc<Vec<Item>>; 3],
    pub loaded: [bool; 3],
    pub loading: [bool; 3],
    pub lib_err: [Option<String>; 3],
    pub groups: [Vec<(String, usize)>; 3],
    pub kind: Kind,
    pub group: Group,
    pub query: String,
    pub view: Vec<u32>,
    pub view_dirty: bool,

    pub epg: Epg,
    pub short_epg: HashMap<u64, Vec<Prog>>,
    pub short_pending: HashSet<u64>,

    pub current: Option<Playing>,
    pub series: Option<SeriesView>,
    pub stats: Stats,
    pub status: String,
    pub last_stats: std::time::Instant,

    pub show_settings: bool,
    pub form: Source,
    pub form_xtream: bool,
    pub form_msg: String,
    pub form_busy: bool,

    pub fullscreen: bool,
    pub last_mouse_move: std::time::Instant,
    pub scroll_to: Option<usize>,
    pub volume: f64,
    pub muted: bool,
}

pub fn mpv_dll() -> std::path::PathBuf {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    let beside = exe.join("libmpv-2.dll");
    if beside.exists() { beside } else { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("libs").join("mpv").join("libmpv-2.dll") }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let (tx, rx) = channel();
        let ctx = cc.egui_ctx.clone();
        let settings: Settings = cache_read::<Settings>("settings").map(|x| x.1).unwrap_or_default();
        let (mpv, mpv_err) = match cc.get_proc_address.clone() {
            Some(pa) => {
                let wake_ctx = ctx.clone();
                let pa2: crate::mpv::ProcAddr = Arc::new(move |name| pa(name));
                match Mpv::new(&mpv_dll(), pa2, Box::new(move || wake_ctx.request_repaint())) {
                    Ok(m) => (Some(m), None),
                    Err(e) => (None, Some(e)),
                }
            }
            None => (None, Some("OpenGL context unavailable".to_string())),
        };
        let volume = settings.volume.unwrap_or(100.0);
        if let Some(m) = &mpv {
            m.set("volume", &format!("{volume}"));
        }
        let src = settings.source.clone();
        let mut app = App {
            tx,
            rx,
            ctx,
            mpv,
            mpv_err,
            video: None,
            force_render: true,
            favs: settings.favs.iter().cloned().collect(),
            recent: settings.recent.clone(),
            form: src.clone(),
            form_xtream: src.xtream || src.playlist.is_empty(),
            src,
            settings,
            libs: [Arc::new(vec![]), Arc::new(vec![]), Arc::new(vec![])],
            loaded: [false; 3],
            loading: [false; 3],
            lib_err: [None, None, None],
            groups: [vec![], vec![], vec![]],
            kind: Kind::Live,
            group: Group::All,
            query: String::new(),
            view: vec![],
            view_dirty: true,
            epg: Epg::default(),
            short_epg: HashMap::new(),
            short_pending: HashSet::new(),
            current: None,
            series: None,
            stats: Stats::default(),
            status: String::new(),
            last_stats: std::time::Instant::now(),
            show_settings: false,
            form_msg: String::new(),
            form_busy: false,
            fullscreen: false,
            last_mouse_move: std::time::Instant::now(),
            scroll_to: None,
            volume,
            muted: false,
        };
        app.load_caches();
        if app.src.xtream && app.src.server.is_empty() || (!app.src.xtream && app.src.playlist.is_empty()) {
            app.show_settings = true;
        }
        app
    }

    pub fn is_xtream(&self) -> bool {
        self.src.xtream && !self.src.server.is_empty()
    }

    pub fn save_settings(&mut self) {
        self.settings.source = self.src.clone();
        self.settings.favs = self.favs.iter().cloned().collect();
        self.settings.recent = self.recent.clone();
        self.settings.volume = Some(self.volume);
        cache_write("settings", &self.settings);
    }

    fn load_caches(&mut self) {
        for k in Kind::ALL {
            if let Some((ts, items)) = cache_read::<Vec<Item>>(&format!("lib-{}", k.slug())) {
                let mut items = items;
                items.iter_mut().for_each(|i| i.prepare());
                self.set_library(k, items);
                if now_ms() - ts > 12 * 3600_000 && k == Kind::Live && (self.is_xtream() || !self.src.playlist.is_empty()) {
                    self.refresh(k);
                }
            }
        }
        if let Some((ts, epg)) = cache_read::<Epg>("epg") {
            self.epg = epg;
            if now_ms() - ts > 12 * 3600_000 && !self.src.epg.is_empty() {
                self.load_epg_url(self.src.epg.clone());
            }
        }
        if !self.loaded[0] && (self.is_xtream() || !self.src.playlist.is_empty()) {
            self.refresh(Kind::Live);
        }
    }

    pub fn set_library(&mut self, kind: Kind, items: Vec<Item>) {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for i in &items {
            *counts.entry(i.group.as_str()).or_default() += 1;
        }
        let mut g: Vec<(String, usize)> = counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        g.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        self.groups[kind.idx()] = g;
        self.libs[kind.idx()] = Arc::new(items);
        self.loaded[kind.idx()] = true;
        self.loading[kind.idx()] = false;
        self.lib_err[kind.idx()] = None;
        if kind == self.kind {
            self.group = Group::All;
            self.view_dirty = true;
        }
    }

    /// (Re)load a library from the configured source in the background.
    pub fn refresh(&mut self, kind: Kind) {
        if self.loading[kind.idx()] {
            return;
        }
        self.loading[kind.idx()] = true;
        self.lib_err[kind.idx()] = None;
        let (tx, ctx, src) = (self.tx.clone(), self.ctx.clone(), self.src.clone());
        std::thread::spawn(move || {
            let msg = if src.xtream {
                match xtream_library(&src.server, &src.user, &src.pass, kind) {
                    Ok(items) => Msg::Library { kind, items, fresh: true },
                    Err(err) => Msg::LibraryFailed { kind, err },
                }
            } else if kind == Kind::Live {
                match load_m3u(&src.playlist) {
                    Ok(m) => {
                        if src.epg.is_empty() && !m.epg_url.is_empty() {
                            if let Ok(e) = load_epg(&m.epg_url) {
                                let _ = tx.send(Msg::Epg(e));
                            }
                        }
                        Msg::Library { kind, items: m.items, fresh: true }
                    }
                    Err(err) => Msg::LibraryFailed { kind, err },
                }
            } else {
                Msg::LibraryFailed { kind, err: "Movies and Series need an Xtream login".into() }
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    pub fn load_epg_url(&self, url: String) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            match load_epg(&url) {
                Ok(e) => {
                    let _ = tx.send(Msg::Epg(e));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Note(format!("Guide failed: {e}")));
                }
            }
            ctx.request_repaint();
        });
    }

    /// Validate the form, resolve the server and replace the active source.
    pub fn connect(&mut self) {
        let mut f = self.form.clone();
        f.xtream = self.form_xtream;
        if f.xtream {
            if f.server.trim().is_empty() || f.user.trim().is_empty() || f.pass.is_empty() {
                self.form_msg = "Enter server, username and password.".into();
                return;
            }
        } else if f.playlist.trim().is_empty() {
            self.form_msg = "Enter a playlist URL or file.".into();
            return;
        }
        self.form_busy = true;
        self.form_msg = "Connecting…".into();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let msg = if f.xtream {
                match xtream_login(&f.server, f.user.trim(), &f.pass) {
                    Ok(base) => Msg::Connected { src: Source { xtream: true, server: base, user: f.user.trim().into(), pass: f.pass.clone(), ..Default::default() } },
                    Err(e) => Msg::ConnectFailed(e),
                }
            } else {
                Msg::Connected { src: Source { xtream: false, playlist: f.playlist.trim().into(), epg: f.epg.trim().into(), ..Default::default() } }
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    pub fn drain_messages(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Connected { src } => {
                    self.src = src.clone();
                    self.form = src.clone();
                    self.form_busy = false;
                    self.show_settings = false;
                    self.form_msg.clear();
                    self.loaded = [false; 3];
                    self.libs = [Arc::new(vec![]), Arc::new(vec![]), Arc::new(vec![])];
                    self.groups = [vec![], vec![], vec![]];
                    self.epg = Epg::default();
                    self.short_epg.clear();
                    self.series = None;
                    self.kind = Kind::Live;
                    self.view_dirty = true;
                    self.save_settings();
                    self.refresh(Kind::Live);
                    if !src.epg.is_empty() {
                        self.load_epg_url(src.epg.clone());
                    }
                }
                Msg::ConnectFailed(e) => {
                    self.form_busy = false;
                    self.form_msg = e;
                }
                Msg::Library { kind, items, fresh } => {
                    if fresh {
                        cache_write(&format!("lib-{}", kind.slug()), &items);
                    }
                    self.set_library(kind, items);
                }
                Msg::LibraryFailed { kind, err } => {
                    self.loading[kind.idx()] = false;
                    self.lib_err[kind.idx()] = Some(err);
                }
                Msg::Epg(e) => {
                    cache_write("epg", &e);
                    self.epg = e;
                }
                Msg::ShortEpg { sid, progs } => {
                    self.short_pending.remove(&sid);
                    self.short_epg.insert(sid, progs);
                }
                Msg::Series { id, info } => {
                    if let Some(s) = &mut self.series {
                        if s.item.series_id == id {
                            s.info = Some(Arc::new(info));
                        }
                    }
                }
                Msg::SeriesFailed { id, err } => {
                    if let Some(s) = &mut self.series {
                        if s.item.series_id == id {
                            s.err = Some(err);
                        }
                    }
                }
                Msg::Note(n) => self.status = n,
            }
        }
    }

    pub fn open_kind(&mut self, kind: Kind) {
        if kind == self.kind {
            return;
        }
        self.kind = kind;
        self.group = Group::All;
        self.query.clear();
        self.series = None;
        self.view_dirty = true;
        self.view.clear();
        if !self.loaded[kind.idx()] && !self.loading[kind.idx()] {
            self.refresh(kind);
        }
    }

    pub fn recompute_view(&mut self) {
        let lib = self.libs[self.kind.idx()].clone();
        let terms: Vec<String> = self.query.to_lowercase().split_whitespace().map(String::from).collect();
        let mut out: Vec<u32> = Vec::new();
        match &self.group {
            Group::Recent => {
                let pos: HashMap<&str, usize> = lib.iter().enumerate().map(|(i, it)| (it.url.as_str(), i)).collect();
                let by_series: HashMap<String, usize> = lib.iter().enumerate().filter(|(_, it)| it.series_id != 0).map(|(i, it)| (it.key(), i)).collect();
                for k in &self.recent {
                    if let Some(&i) = pos.get(k.as_str()).or_else(|| by_series.get(k)) {
                        out.push(i as u32);
                    }
                }
                out.retain(|&i| terms.iter().all(|t| lib[i as usize].lc.contains(t.as_str())));
            }
            g => {
                for (i, it) in lib.iter().enumerate() {
                    let ok = match g {
                        Group::All => true,
                        Group::Fav => self.favs.contains(&it.key()),
                        Group::Named(n) => &it.group == n,
                        Group::Recent => false,
                    };
                    if ok && terms.iter().all(|t| it.lc.contains(t.as_str())) {
                        out.push(i as u32);
                    }
                }
            }
        }
        self.view = out;
        self.view_dirty = false;
    }

    pub fn toggle_fav(&mut self, it: &Item) {
        let k = it.key();
        if !self.favs.remove(&k) {
            self.favs.insert(k);
        }
        self.save_settings();
        if self.group == Group::Fav {
            self.view_dirty = true;
        }
    }

    /* ---------------- playback ---------------- */

    pub fn open_item(&mut self, it: &Item) {
        if it.series_id != 0 {
            self.series = Some(SeriesView { item: it.clone(), info: None, err: None });
            let (tx, ctx, src, id) = (self.tx.clone(), self.ctx.clone(), self.src.clone(), it.series_id);
            std::thread::spawn(move || {
                let _ = tx.send(match xtream_series(&src.server, &src.user, &src.pass, id) {
                    Ok(info) => Msg::Series { id, info },
                    Err(err) => Msg::SeriesFailed { id, err },
                });
                ctx.request_repaint();
            });
        } else {
            self.series = None;
            self.play_item(it.clone(), self.kind, None, 0);
        }
    }

    pub fn play_item(&mut self, item: Item, kind: Kind, episodes: Option<Arc<Vec<Episode>>>, ep_idx: usize) {
        let title = match &episodes {
            Some(eps) => {
                let e = &eps[ep_idx];
                format!("{} · S{}E{} · {}", item.name, e.season, e.num, e.title)
            }
            None => item.name.clone(),
        };
        let url = match &episodes {
            Some(eps) => eps[ep_idx].url.clone(),
            None => item.url.clone(),
        };
        if let Some(m) = &self.mpv {
            m.load(&url);
        }
        self.status = "Loading…".into();
        self.stats = Stats::default();
        let key = item.key();
        self.recent.retain(|k| k != &key);
        self.recent.insert(0, key);
        self.recent.truncate(30);
        if kind == Kind::Live && item.sid != 0 && self.is_xtream() && !self.short_epg.contains_key(&item.sid) && self.short_pending.insert(item.sid) {
            let (tx, ctx, src, sid) = (self.tx.clone(), self.ctx.clone(), self.src.clone(), item.sid);
            std::thread::spawn(move || {
                let progs = xtream_short_epg(&src.server, &src.user, &src.pass, sid).unwrap_or_default();
                let _ = tx.send(Msg::ShortEpg { sid, progs });
                ctx.request_repaint();
            });
        }
        self.current = Some(Playing { item, kind, episodes, ep_idx, title, retried: false });
        self.save_settings();
        self.ctx.request_repaint();
    }

    pub fn play_episode(&mut self, idx: usize) {
        if let Some(s) = &self.series {
            if let Some(info) = &s.info {
                let (item, eps) = (s.item.clone(), Arc::new(info.episodes.clone()));
                self.play_item(item, Kind::Series, Some(eps), idx);
            }
        }
    }

    /// Step to previous/next channel in the current list, or episode in a series.
    pub fn step(&mut self, d: i32) {
        if let Some(cur) = &self.current {
            if let Some(eps) = &cur.episodes {
                let j = cur.ep_idx as i32 + d;
                if j >= 0 && (j as usize) < eps.len() {
                    let (item, eps) = (cur.item.clone(), eps.clone());
                    self.play_item(item, Kind::Series, Some(eps), j as usize);
                }
                return;
            }
        }
        if self.view.is_empty() {
            return;
        }
        let lib = self.libs[self.kind.idx()].clone();
        let cur_url = self.current.as_ref().map(|c| c.item.url.clone());
        let pos = cur_url.and_then(|u| self.view.iter().position(|&i| lib[i as usize].url == u));
        let n = self.view.len() as i32;
        let next = match pos {
            Some(p) => (p as i32 + d).rem_euclid(n) as usize,
            None => 0,
        };
        let it = lib[self.view[next] as usize].clone();
        if it.series_id == 0 {
            self.scroll_to = Some(next);
            self.play_item(it, self.kind, None, 0);
        }
    }

    pub fn stop(&mut self) {
        if let Some(m) = &self.mpv {
            m.stop();
        }
        self.current = None;
        self.status.clear();
        self.stats = Stats::default();
    }

    /// Poll mpv for state and events. Cheap; called every frame.
    pub fn poll_player(&mut self) {
        if self.current.is_none() && self.loaded[0] {
            if let Ok(q) = std::env::var("IPTV_AUTOPLAY") {
                let lib = self.libs[0].clone();
                if let Some(it) = lib.iter().find(|i| i.lc.contains(&q.to_lowercase())) {
                    self.play_item(it.clone(), Kind::Live, None, 0);
                }
            }
        }
        let Some(m) = &self.mpv else { return };
        let mut ended = false;
        let mut failed: Option<String> = None;
        for e in m.poll_events() {
            match e {
                Event_::Loaded => self.status.clear(),
                Event_::Ended => ended = true,
                Event_::Failed(s) => failed = Some(s),
            }
        }
        if self.current.is_some() && self.last_stats.elapsed().as_millis() > 100 {
            self.last_stats = std::time::Instant::now();
            let paused = m.get_bool("pause").unwrap_or(false);
            self.stats = Stats {
                pos: m.get_f64("time-pos").unwrap_or(0.0),
                dur: m.get_f64("duration").unwrap_or(0.0),
                paused,
                buffering: m.get_bool("paused-for-cache").unwrap_or(false) || (!paused && m.get_bool("core-idle").unwrap_or(false)),
                w: m.get_i64("video-params/w").unwrap_or(0),
                h: m.get_i64("video-params/h").unwrap_or(0),
            };
            if self.stats.w > 0 && self.status == "Loading…" {
                self.status.clear();
            }
        }
        if ended {
            if let Some(c) = &self.current {
                if c.episodes.is_some() {
                    self.step(1);
                }
            }
        }
        if let Some(err) = failed {
            let retry = self.current.as_mut().filter(|c| !c.retried && c.kind == Kind::Live).map(|c| {
                c.retried = true;
                c.item.url.clone()
            });
            match retry {
                Some(u) => {
                    if let Some(m) = &self.mpv {
                        m.load(&u);
                    }
                }
                None => self.status = format!("Cannot play this stream ({err})"),
            }
        }
    }

    pub fn is_vod(&self) -> bool {
        self.current.as_ref().map(|c| c.kind != Kind::Live).unwrap_or(false)
    }

    pub fn toggle_pause(&mut self) {
        if let Some(m) = &self.mpv {
            m.command(&["cycle", "pause"]);
        }
    }
    pub fn seek_rel(&mut self, secs: f64) {
        if let Some(m) = &self.mpv {
            m.command(&["seek", &secs.to_string(), "relative"]);
        }
    }
    pub fn seek_abs(&mut self, secs: f64) {
        if let Some(m) = &self.mpv {
            m.command(&["seek", &secs.to_string(), "absolute"]);
        }
    }
    pub fn set_volume(&mut self, v: f64) {
        self.volume = v.clamp(0.0, 130.0);
        if let Some(m) = &self.mpv {
            m.set("volume", &self.volume.to_string());
        }
    }
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        if let Some(m) = &self.mpv {
            m.set("mute", if self.muted { "yes" } else { "no" });
        }
    }

    /* ---------------- EPG lookups ---------------- */

    pub fn programs_for(&self, it: &Item) -> Option<Vec<Prog>> {
        let conv = |v: &Vec<(i64, i64, String, String)>| v.iter().map(|p| Prog { start: p.0, stop: p.1, title: p.2.clone(), desc: p.3.clone() }).collect();
        if !it.epg_id.is_empty() {
            if let Some(v) = self.epg.programs.get(&it.epg_id) {
                return Some(conv(v));
            }
        }
        if !self.epg.names.is_empty() {
            if let Some(id) = self.epg.names.get(&norm_name(&it.name)) {
                if let Some(v) = self.epg.programs.get(id) {
                    return Some(conv(v));
                }
            }
        }
        self.short_epg.get(&it.sid).filter(|v| !v.is_empty()).cloned()
    }

    /// Current programme (title, start, stop) for list rows, without cloning whole lists.
    pub fn now_playing(&self, it: &Item, now: i64) -> Option<(String, i64, i64)> {
        let pick = |v: &Vec<(i64, i64, String, String)>| v.iter().find(|p| p.0 <= now && p.1 > now).map(|p| (p.2.clone(), p.0, p.1));
        if !it.epg_id.is_empty() {
            if let Some(v) = self.epg.programs.get(&it.epg_id) {
                return pick(v);
            }
        }
        if !self.epg.names.is_empty() {
            if let Some(v) = self.epg.names.get(&norm_name(&it.name)).and_then(|id| self.epg.programs.get(id)) {
                return pick(v);
            }
        }
        self.short_epg.get(&it.sid).and_then(|v| v.iter().find(|p| p.start <= now && p.stop > now).map(|p| (p.title.clone(), p.start, p.stop)))
    }

    /* ---------------- video texture ---------------- */

    /// Render the current mpv frame into an offscreen texture sized to the video area.
    pub fn render_video(&mut self, frame: &mut eframe::Frame, px_w: i32, px_h: i32) -> Option<egui::TextureId> {
        let gl = frame.gl()?.clone();
        let (px_w, px_h) = (px_w.max(16), px_h.max(16));
        let needs_new = self.video.as_ref().map(|v| v.w != px_w || v.h != px_h).unwrap_or(true);
        if needs_new {
            unsafe {
                if let Some(old) = self.video.take() {
                    gl.delete_framebuffer(old.fbo);
                    gl.delete_texture(old.tex);
                }
                let tex = gl.create_texture().ok()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, px_w, px_h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                let fbo = gl.create_framebuffer().ok()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                let id = frame.register_native_glow_texture(tex);
                self.video = Some(VideoTex { fbo, tex, id, w: px_w, h: px_h });
            }
            self.force_render = true;
        }
        let v = self.video.as_ref()?;
        if let Some(m) = &self.mpv {
            use glow::NativeFramebuffer;
            let fbo_id: u32 = {
                let NativeFramebuffer(n) = v.fbo;
                n.get()
            };
            if m.render(fbo_id, v.w, v.h, self.force_render) {
                self.force_render = false;
            }
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            }
        }
        Some(v.id)
    }
}
