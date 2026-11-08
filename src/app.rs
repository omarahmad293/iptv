//! Application state and logic (no drawing).
use crate::data::*;
use crate::mpv::{Event_, Mpv, Track};
use eframe::glow;
use eframe::glow::HasContext;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum Msg {
    Connected { src: Source },
    ConnectFailed(String),
    Library { kind: Kind, items: Vec<Item> },
    Cached { epoch: u64, kind: Kind, data: Option<(i64, Vec<Item>)> },
    CachedEpg { epoch: u64, data: Option<(i64, Epg)> },
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
    #[serde(default)]
    pub saved: Vec<Source>,
    pub favs: Vec<String>,
    pub recent: Vec<String>,
    pub volume: Option<f64>,
    /// Window x, y, width, height in points.
    #[serde(default)]
    pub win: Option<[f32; 4]>,
    #[serde(default)]
    pub win_maximized: bool,
    /// Hardware decoding; `None` means the default (on).
    #[serde(default)]
    pub hwdec: Option<bool>,
    /// Parental-control PIN (the settings file is DPAPI-encrypted). Empty = no lock.
    #[serde(default)]
    pub pin: String,
    /// Last playback position (seconds) per movie / episode URL.
    #[serde(default)]
    pub positions: HashMap<String, f64>,
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
    pub retries: u32,
    pub retry_at: Option<std::time::Instant>,
}

pub const MAX_RETRIES: u32 = 5;

/// (label, mpv video-aspect-override, panscan)
pub const ASPECTS: [(&str, &str, &str); 5] = [("Auto", "-1", "0"), ("16:9", "16:9", "0"), ("4:3", "4:3", "0"), ("21:9", "21:9", "0"), ("Fill (crop)", "-1", "1")];
pub const SPEEDS: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

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

pub const MAX_SLOTS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MultiLayout {
    Grid,
    Columns,
    Rows,
    Focus,
}

impl MultiLayout {
    pub const ALL: [MultiLayout; 4] = [MultiLayout::Columns, MultiLayout::Rows, MultiLayout::Focus, MultiLayout::Grid];
    pub fn label(self) -> &'static str {
        match self {
            MultiLayout::Grid => "Grid",
            MultiLayout::Columns => "Side by side",
            MultiLayout::Rows => "Stacked",
            MultiLayout::Focus => "Focus",
        }
    }
}

/// One independent player (own mpv instance, render target and playback state).
pub struct Slot {
    pub mpv: Option<Mpv>,
    pub video: Option<VideoTex>,
    pub force_render: bool,
    pub current: Option<Playing>,
    pub stats: Stats,
    pub status: String,
    pub last_stats: std::time::Instant,
    pub tracks: Vec<Track>,
    pub resume_to: Option<f64>,
}

impl Slot {
    fn new(mpv: Option<Mpv>) -> Slot {
        Slot { mpv, video: None, force_render: true, current: None, stats: Stats::default(), status: String::new(), last_stats: std::time::Instant::now(), tracks: vec![], resume_to: None }
    }
}

fn new_mpv(pa: &crate::mpv::ProcAddr, ctx: &egui::Context, volume: f64, mute: bool, hwdec: bool) -> Result<Mpv, String> {
    let wake_ctx = ctx.clone();
    let m = Mpv::new(&mpv_dll(), pa.clone(), Box::new(move || wake_ctx.request_repaint()))?;
    m.set("volume", &format!("{volume}"));
    m.set("mute", if mute { "yes" } else { "no" });
    m.set("hwdec", if hwdec { "auto-safe" } else { "no" });
    Ok(m)
}

pub struct App {
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    pub ctx: egui::Context,
    pub proc_addr: Option<crate::mpv::ProcAddr>,
    pub slots: Vec<Slot>,
    pub active: usize,
    pub multi: bool,
    /// Number of tiles used in multi-view (2..=MAX_SLOTS).
    pub multi_n: usize,
    pub layout: MultiLayout,
    pub mpv_err: Option<String>,

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
    pub browsing: bool,
    pub group: Group,
    pub query: String,
    pub view: Vec<u32>,
    pub view_dirty: bool,

    pub epg: Epg,
    pub short_epg: HashMap<u64, Vec<Prog>>,
    pub short_pending: HashSet<u64>,

    pub series: Option<SeriesView>,

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

    /// Bumped when the source changes so stale background cache loads are ignored.
    pub epoch: u64,
    pub hwdec: bool,
    pub aspect: usize,
    pub speed: f64,
    pub pip: bool,
    /// Whether the system is currently told to stay awake.
    pub awake: bool,
    /// Parental lock: adult categories are hidden while true.
    pub locked: bool,
    pub pin_input: String,
    pub pin_msg: String,
    /// Digits typed for direct channel-number entry.
    pub chan_buf: String,
    pub chan_at: std::time::Instant,
}

/// Ask Windows not to sleep or blank the screen (call from the UI thread; the flag is per-thread).
pub fn set_keep_awake(on: bool) {
    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }
    let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED } else { ES_CONTINUOUS };
    unsafe {
        SetThreadExecutionState(flags);
    }
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
        let volume = settings.volume.unwrap_or(100.0);
        let hwdec = settings.hwdec.unwrap_or(true);
        let locked = !settings.pin.is_empty();
        let proc_addr: Option<crate::mpv::ProcAddr> = cc.get_proc_address.clone().map(|pa| Arc::new(move |name: &std::ffi::CStr| pa(name)) as crate::mpv::ProcAddr);
        let (mpv, mpv_err) = match &proc_addr {
            Some(pa) => match new_mpv(pa, &ctx, volume, false, hwdec) {
                Ok(m) => (Some(m), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, Some("OpenGL context unavailable".to_string())),
        };
        let src = settings.source.clone();
        let mut app = App {
            tx,
            rx,
            ctx,
            proc_addr,
            slots: vec![Slot::new(mpv)],
            active: 0,
            multi: false,
            multi_n: 2,
            layout: MultiLayout::Columns,
            mpv_err,
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
            browsing: true,
            group: Group::All,
            query: String::new(),
            view: vec![],
            view_dirty: true,
            epg: Epg::default(),
            short_epg: HashMap::new(),
            short_pending: HashSet::new(),
            series: None,
            show_settings: false,
            form_msg: String::new(),
            form_busy: false,
            fullscreen: false,
            last_mouse_move: std::time::Instant::now(),
            scroll_to: None,
            volume,
            muted: false,
            epoch: 0,
            hwdec,
            aspect: 0,
            speed: 1.0,
            pip: false,
            awake: false,
            locked,
            pin_input: String::new(),
            pin_msg: String::new(),
            chan_buf: String::new(),
            chan_at: std::time::Instant::now(),
        };
        app.ensure_saved();
        app.load_caches();
        if app.src.xtream && app.src.server.is_empty() || (!app.src.xtream && app.src.playlist.is_empty()) {
            app.show_settings = true;
        }
        app
    }

    /// Keep the display and system awake while a stream is actually playing.
    pub fn update_awake(&mut self) {
        let want = self.slots.iter().any(|s| s.current.is_some() && !s.stats.paused);
        if want != self.awake {
            self.awake = want;
            set_keep_awake(want);
        }
    }

    pub fn is_xtream(&self) -> bool {
        self.src.xtream && !self.src.server.is_empty()
    }

    /// Migrate the single legacy source into the saved list.
    fn ensure_saved(&mut self) {
        let s = &self.settings.source;
        let has = (s.xtream && !s.server.is_empty()) || (!s.xtream && !s.playlist.is_empty());
        if has && !self.settings.saved.contains(s) {
            let s = s.clone();
            self.settings.saved.push(s);
        }
    }

    pub fn save_settings(&mut self) {
        self.settings.source = self.src.clone();
        self.settings.favs = self.favs.iter().cloned().collect();
        self.settings.recent = self.recent.clone();
        self.settings.volume = Some(self.volume);
        self.settings.hwdec = Some(self.hwdec);
        cache_write("settings", &self.settings);
    }

    fn has_source(&self) -> bool {
        self.is_xtream() || (!self.src.xtream && !self.src.playlist.is_empty())
    }

    /// Read cached libraries and guide on background threads so startup never blocks on disk/JSON/DPAPI.
    fn load_caches(&mut self) {
        for k in Kind::ALL {
            let (tx, ctx, epoch) = (self.tx.clone(), self.ctx.clone(), self.epoch);
            std::thread::spawn(move || {
                let data = cache_read::<Vec<Item>>(&format!("lib-{}", k.slug())).map(|(ts, mut items)| {
                    items.iter_mut().for_each(|i| i.prepare());
                    (ts, items)
                });
                let _ = tx.send(Msg::Cached { epoch, kind: k, data });
                ctx.request_repaint();
            });
        }
        let (tx, ctx, epoch) = (self.tx.clone(), self.ctx.clone(), self.epoch);
        std::thread::spawn(move || {
            let _ = tx.send(Msg::CachedEpg { epoch, data: cache_read::<Epg>("epg") });
            ctx.request_repaint();
        });
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
                    Ok(items) => {
                        cache_write(&format!("lib-{}", kind.slug()), &items);
                        Msg::Library { kind, items }
                    }
                    Err(err) => Msg::LibraryFailed { kind, err },
                }
            } else if kind == Kind::Live {
                match load_m3u(&src.playlist) {
                    Ok(m) => {
                        if src.epg.is_empty() && !m.epg_url.is_empty() {
                            if let Ok(e) = load_epg(&m.epg_url) {
                                cache_write("epg", &e);
                                let _ = tx.send(Msg::Epg(e));
                            }
                        }
                        cache_write(&format!("lib-{}", kind.slug()), &m.items);
                        Msg::Library { kind, items: m.items }
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
                    cache_write("epg", &e);
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
                    let same = |a: &Source| a.xtream == src.xtream && a.server == src.server && a.user == src.user && a.playlist == src.playlist;
                    match self.settings.saved.iter().position(same) {
                        Some(i) => self.settings.saved[i] = src.clone(),
                        None => self.settings.saved.push(src.clone()),
                    }
                    self.src = src.clone();
                    self.form = src.clone();
                    self.form_busy = false;
                    self.show_settings = false;
                    self.form_msg.clear();
                    self.loaded = [false; 3];
                    self.epoch += 1;
                    self.libs = [Arc::new(vec![]), Arc::new(vec![]), Arc::new(vec![])];
                    self.groups = [vec![], vec![], vec![]];
                    self.epg = Epg::default();
                    self.short_epg.clear();
                    self.series = None;
                    self.kind = Kind::Live;
                    self.browsing = true;
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
                Msg::Library { kind, items } => self.set_library(kind, items),
                Msg::Cached { epoch, kind, data } => {
                    if epoch != self.epoch || self.loaded[kind.idx()] {
                        continue;
                    }
                    let stale = match data {
                        Some((ts, items)) => {
                            self.set_library(kind, items);
                            now_ms() - ts > 12 * 3600_000
                        }
                        None => true,
                    };
                    if stale && kind == Kind::Live && self.has_source() {
                        self.refresh(kind);
                    }
                }
                Msg::CachedEpg { epoch, data } => {
                    if epoch != self.epoch {
                        continue;
                    }
                    if let Some((ts, epg)) = data {
                        if self.epg.programs.is_empty() {
                            self.epg = epg;
                        }
                        if now_ms() - ts > 12 * 3600_000 && !self.src.epg.is_empty() {
                            self.load_epg_url(self.src.epg.clone());
                        }
                    }
                }
                Msg::LibraryFailed { kind, err } => {
                    self.loading[kind.idx()] = false;
                    self.lib_err[kind.idx()] = Some(err);
                }
                Msg::Epg(e) => self.epg = e,
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
                Msg::Note(n) => self.slots[self.active].status = n,
            }
        }
    }

    pub fn open_kind(&mut self, kind: Kind) {
        if kind == self.kind {
            return;
        }
        self.kind = kind;
        self.browsing = true;
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
        let hide = |it: &Item| self.locked && is_adult(&it.group);
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
                out.retain(|&i| terms.iter().all(|t| lib[i as usize].lc.contains(t.as_str())) && !hide(&lib[i as usize]));
            }
            g => {
                let pass = |fuzzy: bool| -> Vec<u32> {
                    let mut v = Vec::new();
                    for (i, it) in lib.iter().enumerate() {
                        let ok = match g {
                            Group::All => true,
                            Group::Fav => self.favs.contains(&it.key()),
                            Group::Named(n) => &it.group == n,
                            Group::Recent => false,
                        };
                        if ok && !hide(it) && terms.iter().all(|t| if fuzzy { is_subsequence(&it.lc, t) } else { it.lc.contains(t.as_str()) }) {
                            v.push(i as u32);
                        }
                    }
                    v
                };
                out = pass(false);
                // Nothing matched exactly: fall back to a typo-tolerant search.
                if out.is_empty() && self.query.trim().chars().count() >= 3 {
                    out = pass(true);
                }
                if !terms.is_empty() {
                    out.sort_by_key(|&i| search_rank(&lib[i as usize].lc, &terms));
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
        let a = self.active;
        if let Some(m) = &self.slots[a].mpv {
            m.load(&url);
            if self.speed != 1.0 {
                m.set("speed", "1");
                self.speed = 1.0;
            }
        }
        self.slots[a].status = "Loading…".into();
        self.slots[a].stats = Stats::default();
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
        self.slots[a].current = Some(Playing { item, kind, episodes, ep_idx, title, retries: 0, retry_at: None });
        self.slots[a].tracks.clear();
        self.slots[a].resume_to = if kind != Kind::Live { self.settings.positions.get(&url).copied().filter(|p| *p > 10.0) } else { None };
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

    pub fn cur(&self) -> &Slot {
        &self.slots[self.active]
    }

    /// Step to previous/next channel in the current list, or episode in a series.
    pub fn step(&mut self, d: i32) {
        if let Some(cur) = &self.slots[self.active].current {
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
        let cur_url = self.cur().current.as_ref().map(|c| c.item.url.clone());
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

    /// Play the nth (1-based) entry of the current list.
    pub fn play_number(&mut self, n: usize) {
        let lib = self.libs[self.kind.idx()].clone();
        if let Some(it) = n.checked_sub(1).and_then(|i| self.view.get(i)).and_then(|&i| lib.get(i as usize)).map(|it| it.clone()) {
            self.scroll_to = Some(n - 1);
            self.open_item(&it);
        }
    }

    /* ---------------- parental control ---------------- */

    pub fn try_unlock(&mut self) {
        if self.pin_input == self.settings.pin {
            self.locked = false;
            self.pin_msg.clear();
            self.view_dirty = true;
        } else {
            self.pin_msg = "Wrong PIN".into();
        }
        self.pin_input.clear();
    }

    pub fn lock_now(&mut self) {
        if self.settings.pin.is_empty() {
            return;
        }
        self.locked = true;
        self.view_dirty = true;
        if matches!(&self.group, Group::Named(n) if is_adult(n)) {
            self.group = Group::All;
            self.browsing = true;
        }
        for i in 0..self.slots.len() {
            if self.slots[i].current.as_ref().is_some_and(|c| is_adult(&c.item.group)) {
                self.stop_slot(i);
            }
        }
    }

    pub fn set_pin(&mut self) {
        let p = self.pin_input.trim().to_string();
        if !(4..=8).contains(&p.len()) || !p.chars().all(|c| c.is_ascii_digit()) {
            self.pin_msg = "PIN must be 4-8 digits".into();
            return;
        }
        self.settings.pin = p;
        self.locked = false;
        self.pin_input.clear();
        self.pin_msg = "PIN saved".into();
        self.save_settings();
    }

    pub fn remove_pin(&mut self) {
        self.settings.pin.clear();
        self.locked = false;
        self.pin_msg.clear();
        self.view_dirty = true;
        self.save_settings();
    }

    pub fn stop(&mut self) {
        let s = &mut self.slots[self.active];
        if let Some(m) = &s.mpv {
            m.stop();
        }
        s.current = None;
        s.status.clear();
        s.stats = Stats::default();
        self.save_settings();
    }

    /// Enter or leave multi-view (several streams side by side).
    pub fn set_multi(&mut self, on: bool) {
        if on == self.multi {
            return;
        }
        if on {
            let Some(pa) = self.proc_addr.clone() else { return };
            while self.slots.len() < self.multi_n {
                match new_mpv(&pa, &self.ctx, self.volume, true, self.hwdec) {
                    Ok(m) => self.slots.push(Slot::new(Some(m))),
                    Err(e) => {
                        self.mpv_err = Some(e);
                        break;
                    }
                }
            }
            self.multi = true;
        } else {
            let keep = self.slots.swap_remove(self.active);
            self.slots.clear();
            self.slots.push(keep);
            self.active = 0;
            self.multi = false;
        }
        self.apply_audio();
        self.force_all_render();
    }

    /// Change the number of multi-view tiles while multi-view is on.
    pub fn set_multi_count(&mut self, n: usize) {
        let n = n.clamp(2, MAX_SLOTS);
        self.multi_n = n;
        if !self.multi {
            return;
        }
        if let Some(pa) = self.proc_addr.clone() {
            while self.slots.len() < n {
                match new_mpv(&pa, &self.ctx, self.volume, true, self.hwdec) {
                    Ok(m) => self.slots.push(Slot::new(Some(m))),
                    Err(e) => {
                        self.mpv_err = Some(e);
                        break;
                    }
                }
            }
        }
        while self.slots.len() > n {
            let last = self.slots.len() - 1;
            if self.active == last {
                self.slots.swap(last, 0);
                self.active = 0;
            }
            self.slots.pop();
        }
        self.apply_audio();
        self.force_all_render();
    }

    pub fn force_all_render(&mut self) {
        for s in &mut self.slots {
            s.force_render = true;
        }
    }

    pub fn set_active(&mut self, i: usize) {
        if i < self.slots.len() && i != self.active {
            self.active = i;
            self.apply_audio();
        }
    }

    /// Only the active player is audible.
    pub fn apply_audio(&mut self) {
        for (i, s) in self.slots.iter().enumerate() {
            if let Some(m) = &s.mpv {
                m.set("mute", if i == self.active && !self.muted { "no" } else { "yes" });
            }
        }
    }

    pub fn stop_slot(&mut self, i: usize) {
        let prev = std::mem::replace(&mut self.active, i);
        self.stop();
        self.active = prev;
    }

    /// Poll mpv for state and events on every player. Cheap; called every frame.
    pub fn poll_player(&mut self) {
        if self.slots[0].current.is_none() && self.loaded[0] {
            if let Ok(q) = std::env::var("IPTV_AUTOPLAY") {
                let lib = self.libs[0].clone();
                if let Some(it) = lib.iter().find(|i| i.lc.contains(&q.to_lowercase())) {
                    self.play_item(it.clone(), Kind::Live, None, 0);
                }
            }
        }
        for i in 0..self.slots.len() {
            let s = &mut self.slots[i];
            let Some(m) = &s.mpv else { continue };
            let mut ended = false;
            let mut failed: Option<String> = None;
            let mut loaded = false;
            for e in m.poll_events() {
                match e {
                    Event_::Loaded => {
                        s.status.clear();
                        loaded = true;
                    }
                    Event_::Ended => ended = true,
                    Event_::Failed(e) => failed = Some(e),
                }
            }
            if loaded {
                s.tracks = m.tracks();
                if let Some(p) = s.resume_to.take() {
                    m.command(&["seek", &p.to_string(), "absolute"]);
                }
            }
            if s.current.is_some() && s.last_stats.elapsed().as_millis() > 100 {
                s.last_stats = std::time::Instant::now();
                let paused = m.get_bool("pause").unwrap_or(false);
                s.stats = Stats {
                    pos: m.get_f64("time-pos").unwrap_or(0.0),
                    dur: m.get_f64("duration").unwrap_or(0.0),
                    paused,
                    buffering: m.get_bool("paused-for-cache").unwrap_or(false) || (!paused && m.get_bool("core-idle").unwrap_or(false)),
                    w: m.get_i64("video-params/w").unwrap_or(0),
                    h: m.get_i64("video-params/h").unwrap_or(0),
                };
                if s.stats.w > 0 && s.status == "Loading…" {
                    s.status.clear();
                }
                // Remember where movies / episodes were left off.
                if let Some(c) = s.current.as_ref().filter(|c| c.kind != Kind::Live) {
                    if s.stats.dur > 120.0 && s.resume_to.is_none() && !s.status.starts_with("Loading") {
                        let url = c.episodes.as_ref().map(|e| e[c.ep_idx].url.clone()).unwrap_or_else(|| c.item.url.clone());
                        if s.stats.pos >= s.stats.dur * 0.95 {
                            self.settings.positions.remove(&url);
                        } else if s.stats.pos > 10.0 && (s.stats.pos - self.settings.positions.get(&url).copied().unwrap_or(0.0)).abs() >= 5.0 {
                            if self.settings.positions.len() > 300 {
                                let drop: Vec<String> = self.settings.positions.keys().take(50).cloned().collect();
                                drop.iter().for_each(|k| { self.settings.positions.remove(k); });
                            }
                            self.settings.positions.insert(url, s.stats.pos);
                        }
                    }
                }
                // A stream that has stayed up for a while is healthy again.
                if s.stats.pos > 20.0 {
                    if let Some(c) = s.current.as_mut() {
                        c.retries = 0;
                    }
                }
            }
            if ended && s.current.as_ref().map(|c| c.episodes.is_some()).unwrap_or(false) {
                let prev = std::mem::replace(&mut self.active, i);
                self.step(1);
                self.active = prev;
                continue;
            }
            // Live streams that drop or fail are retried with exponential backoff.
            if failed.is_some() || ended {
                if let Some(c) = s.current.as_mut().filter(|c| c.kind == Kind::Live && c.retry_at.is_none()) {
                    if c.retries < MAX_RETRIES {
                        c.retries += 1;
                        c.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(1 << (c.retries - 1)));
                        s.status = format!("Connection lost — reconnecting ({}/{MAX_RETRIES})…", c.retries);
                    } else {
                        s.status = format!("Cannot play this stream ({})", failed.as_deref().unwrap_or("stream ended"));
                    }
                } else if let Some(err) = &failed {
                    if s.current.as_ref().map(|c| c.kind != Kind::Live).unwrap_or(false) {
                        s.status = format!("Cannot play this stream ({err})");
                    }
                }
            }
            if let Some(c) = s.current.as_mut() {
                if c.retry_at.is_some_and(|t| t <= std::time::Instant::now()) {
                    c.retry_at = None;
                    m.load(&c.item.url);
                }
            }
        }
    }

    pub fn retry_now(&mut self) {
        let s = &mut self.slots[self.active];
        if let (Some(m), Some(c)) = (&s.mpv, s.current.as_mut()) {
            c.retries = 0;
            c.retry_at = None;
            s.status = "Loading…".into();
            m.load(&c.item.url);
        }
    }

    pub fn set_audio_track(&mut self, id: i64) {
        self.set_track("aid", id);
    }

    /// `id < 0` turns subtitles off.
    pub fn set_sub_track(&mut self, id: i64) {
        self.set_track("sid", id);
    }

    fn set_track(&mut self, prop: &str, id: i64) {
        let s = &mut self.slots[self.active];
        if let Some(m) = &s.mpv {
            m.set(prop, &if id < 0 { "no".to_string() } else { id.to_string() });
            s.tracks = m.tracks();
        }
    }

    pub fn set_aspect(&mut self, i: usize) {
        self.aspect = i;
        let (_, ar, pan) = ASPECTS[i];
        for s in &self.slots {
            if let Some(m) = &s.mpv {
                m.set("video-aspect-override", ar);
                m.set("panscan", pan);
            }
        }
        self.force_all_render();
    }

    pub fn set_speed(&mut self, v: f64) {
        self.speed = v;
        if let Some(m) = &self.cur().mpv {
            m.set("speed", &v.to_string());
        }
    }

    pub fn set_hwdec(&mut self, on: bool) {
        self.hwdec = on;
        for s in &self.slots {
            if let Some(m) = &s.mpv {
                m.set("hwdec", if on { "auto-safe" } else { "no" });
            }
        }
        self.save_settings();
    }

    pub fn is_vod(&self) -> bool {
        self.cur().current.as_ref().map(|c| c.kind != Kind::Live).unwrap_or(false)
    }

    pub fn toggle_pause(&mut self) {
        if let Some(m) = &self.cur().mpv {
            m.command(&["cycle", "pause"]);
        }
    }
    pub fn seek_rel(&mut self, secs: f64) {
        if let Some(m) = &self.cur().mpv {
            m.command(&["seek", &secs.to_string(), "relative"]);
        }
    }
    pub fn seek_abs(&mut self, secs: f64) {
        if let Some(m) = &self.cur().mpv {
            m.command(&["seek", &secs.to_string(), "absolute"]);
        }
    }
    pub fn set_volume(&mut self, v: f64) {
        self.volume = v.clamp(0.0, 130.0);
        for s in &self.slots {
            if let Some(m) = &s.mpv {
                m.set("volume", &self.volume.to_string());
            }
        }
    }
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.apply_audio();
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
    pub fn render_video(&mut self, slot: usize, frame: &mut eframe::Frame, px_w: i32, px_h: i32) -> Option<egui::TextureId> {
        let gl = frame.gl()?.clone();
        let (px_w, px_h) = (px_w.max(16), px_h.max(16));
        let s = &mut self.slots[slot];
        let needs_new = s.video.as_ref().map(|v| v.w != px_w || v.h != px_h).unwrap_or(true);
        if needs_new {
            let had_old = s.video.is_some();
            let created = unsafe {
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
                VideoTex { fbo, tex, id, w: px_w, h: px_h }
            };
            // Render into the new texture first; keep showing the old (stretched) one until it has a real frame,
            // otherwise a resize (e.g. toggling fullscreen) flashes black.
            let ok = match &s.mpv {
                Some(m) => {
                    use glow::NativeFramebuffer;
                    let NativeFramebuffer(n) = created.fbo;
                    let r = m.render(n.get(), created.w, created.h, true);
                    unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, None) };
                    r
                }
                None => true,
            };
            if ok || !had_old {
                unsafe {
                    if let Some(old) = s.video.take() {
                        gl.delete_framebuffer(old.fbo);
                        gl.delete_texture(old.tex);
                    }
                }
                s.video = Some(created);
                s.force_render = !ok;
            } else {
                unsafe {
                    gl.delete_framebuffer(created.fbo);
                    gl.delete_texture(created.tex);
                }
                self.ctx.request_repaint();
            }
            return s.video.as_ref().map(|v| v.id);
        }
        let v = s.video.as_ref()?;
        if let Some(m) = &s.mpv {
            use glow::NativeFramebuffer;
            let fbo_id: u32 = {
                let NativeFramebuffer(n) = v.fbo;
                n.get()
            };
            if m.render(fbo_id, v.w, v.h, s.force_render) {
                s.force_render = false;
            }
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            }
        }
        Some(v.id)
    }
}
