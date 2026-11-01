//! Data layer: models, M3U / XMLTV / Xtream parsing, HTTP and on-disk cache.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) IPTVPlayer/1.0";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum Kind {
    Live,
    Movie,
    Series,
}
impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Live, Kind::Movie, Kind::Series];
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Live => "Live",
            Kind::Movie => "Movies",
            Kind::Series => "Series",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Live => "live",
            Kind::Movie => "movies",
            Kind::Series => "series",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Item {
    pub name: String,
    pub logo: String,
    pub group: String,
    pub url: String,
    pub epg_id: String,
    pub sid: u64,
    pub series_id: u64,
    pub rating: String,
    pub plot: String,
    #[serde(skip)]
    pub lc: String,
}
impl Item {
    pub fn prepare(&mut self) {
        self.lc = self.name.to_lowercase();
    }
    /// Stable key for favourites / recents.
    pub fn key(&self) -> String {
        if self.series_id != 0 { format!("series:{}", self.series_id) } else { self.url.clone() }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Source {
    pub xtream: bool,
    pub server: String,
    pub user: String,
    pub pass: String,
    pub playlist: String,
    pub epg: String,
}

#[derive(Clone, Debug)]
pub struct Prog {
    pub start: i64,
    pub stop: i64,
    pub title: String,
    pub desc: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Epg {
    pub programs: HashMap<String, Vec<(i64, i64, String, String)>>,
    pub names: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Episode {
    pub season: String,
    pub num: String,
    pub title: String,
    pub plot: String,
    pub duration: String,
    pub url: String,
}
#[derive(Clone, Debug, Default)]
pub struct SeriesInfo {
    pub plot: String,
    pub episodes: Vec<Episode>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/* ---------------- paths / cache ---------------- */

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let d = base.join("IPTV Player Rust");
    let _ = std::fs::create_dir_all(&d);
    d
}

#[derive(Serialize, Deserialize)]
struct CacheFile<T> {
    ts: i64,
    data: T,
}

pub fn cache_write<T: Serialize>(name: &str, data: &T) {
    let f = CacheFile { ts: now_ms(), data };
    if let Ok(s) = serde_json::to_vec(&f) {
        let _ = std::fs::write(data_dir().join(format!("{name}.json")), s);
    }
}
pub fn cache_read<T: for<'de> Deserialize<'de>>(name: &str) -> Option<(i64, T)> {
    let b = std::fs::read(data_dir().join(format!("{name}.json"))).ok()?;
    let f: CacheFile<T> = serde_json::from_slice(&b).ok()?;
    Some((f.ts, f.data))
}

/* ---------------- HTTP ---------------- */

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .user_agent(UA)
        .http_status_as_error(true)
        .build()
        .into()
}

fn http_bytes(url: &str, timeout: u64) -> Result<Vec<u8>, String> {
    let mut resp = agent(timeout).get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut().with_config().limit(1 << 30).read_to_vec().map_err(|e| e.to_string())
}

fn read_source(src: &str) -> Result<String, String> {
    let mut buf = if src.starts_with("http://") || src.starts_with("https://") {
        http_bytes(src, 180)?
    } else {
        std::fs::read(src).map_err(|e| format!("{src}: {e}"))?
    };
    if buf.len() > 2 && buf[0] == 0x1f && buf[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&buf[..]).read_to_end(&mut out).map_err(|e| e.to_string())?;
        buf = out;
    }
    let s = String::from_utf8_lossy(&buf).into_owned();
    Ok(s.trim_start_matches('\u{feff}').to_string())
}

/* ---------------- M3U ---------------- */

pub struct M3u {
    pub items: Vec<Item>,
    pub epg_url: String,
}

fn attr<'a>(head: &'a str, key: &str) -> &'a str {
    let pat = format!("{key}=\"");
    let lower = head.to_ascii_lowercase();
    if let Some(i) = lower.find(&pat) {
        let s = i + pat.len();
        if let Some(e) = head[s..].find('"') {
            return &head[s..s + e];
        }
    }
    ""
}

pub fn parse_m3u(text: &str) -> M3u {
    let mut items = Vec::new();
    let mut epg_url = String::new();
    let mut cur: Option<Item> = None;
    let mut grp = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXTM3U") {
            let u = attr(line, "url-tvg");
            let u = if u.is_empty() { attr(line, "x-tvg-url") } else { u };
            epg_url = u.split(',').next().unwrap_or("").trim().to_string();
        } else if line.starts_with("#EXTINF") {
            let comma = line.rfind(',');
            let head = comma.map(|c| &line[..c]).unwrap_or(line);
            let title = comma.map(|c| line[c + 1..].trim()).unwrap_or("");
            let name = if !title.is_empty() { title } else { attr(head, "tvg-name") };
            let g = attr(head, "group-title");
            cur = Some(Item {
                name: if name.is_empty() { "Unnamed".into() } else { decode_entities(name) },
                logo: attr(head, "tvg-logo").to_string(),
                group: if !g.is_empty() { g.to_string() } else if !grp.is_empty() { grp.clone() } else { "Uncategorized".into() },
                epg_id: attr(head, "tvg-id").to_string(),
                ..Default::default()
            });
        } else if let Some(g) = line.strip_prefix("#EXTGRP:") {
            grp = g.trim().to_string();
        } else if !line.starts_with('#') {
            let mut it = cur.take().unwrap_or_else(|| Item {
                name: line.rsplit('/').next().unwrap_or("Unnamed").to_string(),
                group: if grp.is_empty() { "Uncategorized".into() } else { grp.clone() },
                ..Default::default()
            });
            it.url = line.to_string();
            it.prepare();
            items.push(it);
        }
    }
    M3u { items, epg_url }
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'");
    let mut from = 0;
    while let Some(i) = out[from..].find("&#") {
        let i = from + i;
        let Some(e) = out[i..].find(';') else { break };
        let body = &out[i + 2..i + e];
        let ch = if let Some(h) = body.strip_prefix('x').or(body.strip_prefix('X')) { u32::from_str_radix(h, 16).ok() } else { body.parse().ok() }
            .and_then(char::from_u32);
        match ch {
            Some(c) => {
                let c = c.to_string();
                out.replace_range(i..i + e + 1, &c);
                from = i + c.len();
            }
            None => from = i + 2,
        }
    }
    out.replace("&amp;", "&")
}

/* ---------------- XMLTV ---------------- */

pub fn norm_name(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn xml_time(s: &str) -> Option<i64> {
    let b = s.trim();
    if b.len() < 12 {
        return None;
    }
    let n = |r: std::ops::Range<usize>| b.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi) = (n(0..4)?, n(4..6)?, n(6..8)?, n(8..10)?, n(10..12)?);
    let sec = if b.len() >= 14 { n(12..14).unwrap_or(0) } else { 0 };
    let days = days_from_civil(y, mo, d);
    let mut t = ((days * 24 + h) * 60 + mi) * 60 + sec;
    if let Some(z) = b.get(14..).map(|r| r.trim()).filter(|r| r.len() >= 5) {
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let zh: i64 = z.get(1..3)?.parse().ok()?;
        let zm: i64 = z.get(3..5)?.parse().ok()?;
        t -= sign * (zh * 60 + zm) * 60;
    }
    Some(t * 1000)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn tag_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = block.find(&format!("<{tag}"))?;
    let gt = block[open..].find('>')? + open + 1;
    let end = block[gt..].find('<')? + gt;
    Some(&block[gt..end])
}
fn xml_attr<'a>(head: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!(" {key}=\"");
    let i = head.find(&pat)? + pat.len();
    let e = head[i..].find('"')?;
    Some(&head[i..i + e])
}

pub fn parse_xmltv(text: &str) -> Epg {
    let now = now_ms();
    let (min_stop, max_start) = (now - 3 * 3600_000, now + 72 * 3600_000);
    let mut epg = Epg::default();
    let mut pos = 0;
    while let Some(i) = text[pos..].find("<channel ") {
        let s = pos + i;
        let Some(e) = text[s..].find("</channel>") else { break };
        let block = &text[s..s + e];
        let head_end = block.find('>').unwrap_or(block.len());
        if let (Some(id), Some(dn)) = (xml_attr(&block[..head_end], "id"), tag_text(block, "display-name")) {
            epg.names.insert(norm_name(&decode_entities(dn)), decode_entities(id));
        }
        pos = s + e + 10;
    }
    let mut pos = 0;
    while let Some(i) = text[pos..].find("<programme ") {
        let s = pos + i;
        let Some(e) = text[s..].find("</programme>") else { break };
        let block = &text[s..s + e];
        pos = s + e + 12;
        let head_end = block.find('>').unwrap_or(block.len());
        let head = &block[..head_end];
        let (Some(start), Some(stop), Some(ch)) = (
            xml_attr(head, "start").and_then(xml_time),
            xml_attr(head, "stop").and_then(xml_time),
            xml_attr(head, "channel"),
        ) else {
            continue;
        };
        if stop <= min_stop || start > max_start {
            continue;
        }
        let title = tag_text(block, "title").map(decode_entities).unwrap_or_default();
        let desc: String = tag_text(block, "desc").map(decode_entities).unwrap_or_default().chars().take(600).collect();
        epg.programs.entry(decode_entities(ch)).or_default().push((start, stop, title, desc));
    }
    for v in epg.programs.values_mut() {
        v.sort_by_key(|p| p.0);
    }
    epg
}

/* ---------------- Xtream ---------------- */

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn u(v: &Value, k: &str) -> u64 {
    match v.get(k) {
        Some(Value::String(x)) => x.parse().unwrap_or(0),
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}
fn enc(x: &str) -> String {
    let mut o = String::new();
    for b in x.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn api_json(server: &str, user: &str, pass: &str, action: &str, timeout: u64) -> Result<Value, String> {
    let mut url = format!("{server}/player_api.php?username={}&password={}", enc(user), enc(pass));
    if !action.is_empty() {
        url.push('&');
        url.push_str(action);
    }
    let b = http_bytes(&url, timeout)?;
    serde_json::from_slice(&b).map_err(|e| format!("bad server response: {e}"))
}

/// Resolve a working base URL (tries the given scheme, then the other) and verify the login.
pub fn xtream_login(server: &str, user: &str, pass: &str) -> Result<String, String> {
    let server = server.trim().trim_end_matches('/');
    let server = if server.starts_with("http://") || server.starts_with("https://") { server.to_string() } else { format!("http://{server}") };
    let other = if let Some(r) = server.strip_prefix("https://") { format!("http://{r}") } else { server.replacen("http://", "https://", 1) };
    let mut last = String::new();
    for base in [server, other] {
        match api_json(&base, user, pass, "", 30) {
            Ok(v) => {
                let Some(ui) = v.get("user_info") else { return Err("Login rejected (wrong username or password)".into()) };
                let auth = ui.get("auth").map(|a| a == &Value::from(1) || a == &Value::from("1")).unwrap_or(false);
                if !auth {
                    return Err("Login rejected (wrong username or password)".into());
                }
                let status = s(ui, "status");
                if !status.is_empty() && status != "Active" {
                    return Err(format!("Account status: {status}"));
                }
                return Ok(base);
            }
            Err(e) => last = e,
        }
    }
    Err(format!("Cannot reach server ({last})"))
}

pub fn xtream_library(base: &str, user: &str, pass: &str, kind: Kind) -> Result<Vec<Item>, String> {
    let (cat_act, list_act) = match kind {
        Kind::Live => ("get_live_categories", "get_live_streams"),
        Kind::Movie => ("get_vod_categories", "get_vod_streams"),
        Kind::Series => ("get_series_categories", "get_series"),
    };
    let cats_h = {
        let (b, u, p) = (base.to_string(), user.to_string(), pass.to_string());
        std::thread::spawn(move || api_json(&b, &u, &p, &format!("action={cat_act}"), 120))
    };
    let list = api_json(base, user, pass, &format!("action={list_act}"), 180)?;
    let cats = cats_h.join().map_err(|_| "worker panic".to_string())??;
    let cm: HashMap<String, String> = cats.as_array().map(|a| a.iter().map(|c| (s(c, "category_id"), s(c, "category_name"))).collect()).unwrap_or_default();
    let (eu, ep) = (enc(user), enc(pass));
    let mut out = Vec::new();
    for v in list.as_array().ok_or("unexpected server response")? {
        let name = {
            let n = s(v, "name");
            if n.is_empty() { s(v, "title") } else { n }
        };
        let mut it = Item {
            name: if name.is_empty() { "Unnamed".into() } else { name },
            group: cm.get(&s(v, "category_id")).cloned().unwrap_or_else(|| "Uncategorized".into()),
            rating: s(v, "rating"),
            ..Default::default()
        };
        match kind {
            Kind::Live => {
                it.sid = u(v, "stream_id");
                it.logo = s(v, "stream_icon");
                it.epg_id = s(v, "epg_channel_id");
                it.url = format!("{base}/live/{eu}/{ep}/{}.ts", it.sid);
            }
            Kind::Movie => {
                it.sid = u(v, "stream_id");
                it.logo = s(v, "stream_icon");
                let ext = s(v, "container_extension");
                it.url = format!("{base}/movie/{eu}/{ep}/{}.{}", it.sid, if ext.is_empty() { "mp4" } else { &ext });
            }
            Kind::Series => {
                it.series_id = u(v, "series_id");
                it.logo = s(v, "cover");
                it.plot = s(v, "plot");
            }
        }
        it.prepare();
        out.push(it);
    }
    Ok(out)
}

pub fn xtream_series(base: &str, user: &str, pass: &str, series_id: u64) -> Result<SeriesInfo, String> {
    let j = api_json(base, user, pass, &format!("action=get_series_info&series_id={series_id}"), 60)?;
    let (eu, ep) = (enc(user), enc(pass));
    let mut info = SeriesInfo { plot: j.get("info").map(|i| s(i, "plot")).unwrap_or_default(), episodes: vec![] };
    if let Some(Value::Object(seasons)) = j.get("episodes") {
        let mut keys: Vec<&String> = seasons.keys().collect();
        keys.sort_by_key(|k| k.parse::<i64>().unwrap_or(0));
        for k in keys {
            for e in seasons[k].as_array().into_iter().flatten() {
                let ext = s(e, "container_extension");
                let inf = e.get("info").cloned().unwrap_or(Value::Null);
                let num = s(e, "episode_num");
                let title = s(e, "title");
                info.episodes.push(Episode {
                    season: k.clone(),
                    title: if title.is_empty() { format!("Episode {num}") } else { title },
                    num,
                    plot: s(&inf, "plot"),
                    duration: s(&inf, "duration"),
                    url: format!("{base}/series/{eu}/{ep}/{}.{}", s(e, "id"), if ext.is_empty() { "mp4" } else { &ext }),
                });
            }
        }
    }
    Ok(info)
}

fn b64(x: &str) -> String {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in x.bytes() {
        if let Some(v) = val(c) {
            acc = (acc << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| x.to_string())
}

pub fn xtream_short_epg(base: &str, user: &str, pass: &str, sid: u64) -> Result<Vec<Prog>, String> {
    let j = api_json(base, user, pass, &format!("action=get_short_epg&stream_id={sid}&limit=20"), 20)?;
    let mut v: Vec<Prog> = j
        .get("epg_listings")
        .and_then(|l| l.as_array())
        .into_iter()
        .flatten()
        .map(|e| Prog {
            start: u(e, "start_timestamp") as i64 * 1000,
            stop: u(e, "stop_timestamp") as i64 * 1000,
            title: b64(&s(e, "title")),
            desc: b64(&s(e, "description")).chars().take(600).collect(),
        })
        .collect();
    v.sort_by_key(|p| p.start);
    Ok(v)
}

/* ---------------- M3U / EPG loaders ---------------- */

pub fn load_m3u(src: &str) -> Result<M3u, String> {
    let text = read_source(src)?;
    let m = parse_m3u(&text);
    if m.items.is_empty() {
        return Err("No channels found in playlist".into());
    }
    Ok(m)
}

pub fn load_epg(src: &str) -> Result<Epg, String> {
    Ok(parse_xmltv(&read_source(src)?))
}
