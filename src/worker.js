const { workerData, parentPort } = require('worker_threads');
const fs = require('fs');
const zlib = require('zlib');

const UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) IPTVPlayer/1.0';

async function readSource(src) {
  let buf;
  if (/^https?:\/\//i.test(src)) {
    const res = await fetch(src, { headers: { 'User-Agent': UA }, redirect: 'follow' });
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    buf = Buffer.from(await res.arrayBuffer());
  } else {
    buf = await fs.promises.readFile(src);
  }
  if (buf[0] === 0x1f && buf[1] === 0x8b) buf = zlib.gunzipSync(buf);
  return buf.toString('utf8').replace(/^\uFEFF/, '');
}

function parseM3U(text) {
  const channels = [];
  let epgUrl = '';
  const hm = /^#EXTM3U([^\r\n]*)/.exec(text);
  if (hm) { const m = /(?:url-tvg|x-tvg-url)="([^"]*)"/i.exec(hm[1]); if (m) epgUrl = m[1].split(',')[0].trim(); }
  const attrRe = /([\w-]+)="([^"]*)"/g;
  let cur = null, grp = '';
  let pos = 0;
  const len = text.length;
  while (pos < len) {
    let nl = text.indexOf('\n', pos);
    if (nl < 0) nl = len;
    const line = text.slice(pos, nl).trim();
    pos = nl + 1;
    if (!line) continue;
    if (line.startsWith('#EXTINF')) {
      const comma = line.lastIndexOf(',');
      const head = comma >= 0 ? line.slice(0, comma) : line;
      const a = {};
      attrRe.lastIndex = 0;
      let m;
      while ((m = attrRe.exec(head))) a[m[1].toLowerCase()] = m[2];
      cur = {
        name: dec((comma >= 0 ? line.slice(comma + 1).trim() : '') || a['tvg-name'] || 'Unnamed'),
        logo: a['tvg-logo'] || '',
        group: a['group-title'] || grp || 'Uncategorized',
        epgId: a['tvg-id'] || '',
      };
    } else if (line.startsWith('#EXTGRP:')) {
      grp = line.slice(8).trim();
    } else if (line[0] !== '#') {
      if (!cur) cur = { name: line.split('/').pop(), logo: '', group: grp || 'Uncategorized', epgId: '' };
      cur.url = line;
      cur.id = channels.length;
      channels.push(cur);
      cur = null;
    }
  }
  return { channels, epgUrl };
}

const ENT = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'" };
const dec = (s) => s.indexOf('&') < 0 ? s : s
  .replace(/&(amp|lt|gt|quot|apos);/g, (m) => ENT[m])
  .replace(/&#(x?)([0-9a-f]+);/gi, (_, x, n) => String.fromCodePoint(parseInt(n, x ? 16 : 10)));

function xmlTime(s) {
  // YYYYMMDDHHMMSS [+HHMM]
  const m = /^(\d{4})(\d\d)(\d\d)(\d\d)(\d\d)(\d\d)?\s*([+-]\d{4})?/.exec(s);
  if (!m) return NaN;
  let t = Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +(m[6] || 0));
  if (m[7]) {
    const sign = m[7][0] === '-' ? -1 : 1;
    t -= sign * (+m[7].slice(1, 3) * 60 + +m[7].slice(3, 5)) * 60000;
  }
  return t;
}

const norm = (s) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');

function parseXMLTV(text) {
  const now = Date.now();
  const minStop = now - 3 * 3600e3, maxStart = now + 72 * 3600e3;
  const programs = {}, names = {};
  const chRe = /<channel\s+id="([^"]*)"[^>]*>([\s\S]*?)<\/channel>/g;
  let m;
  while ((m = chRe.exec(text))) {
    const id = dec(m[1]);
    const dn = /<display-name[^>]*>([^<]*)</.exec(m[2]);
    if (dn) names[norm(dec(dn[1]))] = id;
  }
  const pRe = /<programme\s([^>]*)>([\s\S]*?)<\/programme>/g;
  const aRe = /(start|stop|channel)="([^"]*)"/g;
  while ((m = pRe.exec(text))) {
    const at = {};
    let a; aRe.lastIndex = 0;
    while ((a = aRe.exec(m[1]))) at[a[1]] = a[2];
    const start = xmlTime(at.start || ''), stop = xmlTime(at.stop || '');
    if (!(stop > minStop) || start > maxStart) continue;
    const t = /<title[^>]*>([^<]*)</.exec(m[2]);
    const d = /<desc[^>]*>([^<]*)</.exec(m[2]);
    const ch = dec(at.channel || '');
    (programs[ch] || (programs[ch] = [])).push([start, stop, t ? dec(t[1]) : '', d ? dec(d[1]).slice(0, 600) : '']);
  }
  for (const k in programs) programs[k].sort((x, y) => x[0] - y[0]);
  return { programs, names };
}

async function loadXtream({ server, user, pass }) {
  const q = `username=${encodeURIComponent(user)}&password=${encodeURIComponent(pass)}`;
  const get = async (base, action) => {
    const res = await fetch(`${base}/player_api.php?${q}${action ? '&action=' + action : ''}`, { headers: { 'User-Agent': UA }, signal: AbortSignal.timeout(60000) });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  };
  const bases = [server];
  bases.push(server.startsWith('https:') ? server.replace(/^https:/, 'http:') : server.replace(/^http:/, 'https:'));
  let base, info, lastErr;
  for (const b of bases) { try { info = await get(b); base = b; break; } catch (e) { lastErr = e; } }
  if (!base) throw new Error('Cannot reach server (' + (lastErr && lastErr.cause ? lastErr.cause.code || lastErr.cause.message : lastErr.message) + ')');
  if (!info.user_info || info.user_info.auth !== 1) throw new Error('Login rejected (wrong username or password)');
  if (info.user_info.status && info.user_info.status !== 'Active') throw new Error('Account status: ' + info.user_info.status);
  const [cats, streams] = await Promise.all([get(base, 'get_live_categories'), get(base, 'get_live_streams')]);
  const cm = new Map(cats.map((c) => [String(c.category_id), c.category_name]));
  const channels = streams.map((s, i) => ({
    id: i, kind: 'live', name: s.name || 'Unnamed', logo: s.stream_icon || '', group: cm.get(String(s.category_id)) || 'Uncategorized',
    epgId: '', sid: s.stream_id, url: `${base}/live/${encodeURIComponent(user)}/${encodeURIComponent(pass)}/${s.stream_id}.ts`,
  }));
  return { channels, epgUrl: '', base };
}

async function loadXtreamLib({ server, user, pass, kind }) {
  const q = `username=${encodeURIComponent(user)}&password=${encodeURIComponent(pass)}`;
  const get = async (action) => {
    const res = await fetch(`${server}/player_api.php?${q}&action=${action}`, { headers: { 'User-Agent': UA }, signal: AbortSignal.timeout(120000) });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  };
  const [catAct, listAct] = kind === 'movie' ? ['get_vod_categories', 'get_vod_streams'] : ['get_series_categories', 'get_series'];
  const [cats, list] = await Promise.all([get(catAct), get(listAct)]);
  const cm = new Map(cats.map((c) => [String(c.category_id), c.category_name]));
  const u = encodeURIComponent(user), p = encodeURIComponent(pass);
  const channels = list.map((s, i) => {
    const base = { id: i, kind, name: s.name || s.title || 'Unnamed', group: cm.get(String(s.category_id)) || 'Uncategorized', epgId: '', rating: s.rating || '' };
    if (kind === 'movie') return { ...base, logo: s.stream_icon || '', url: `${server}/movie/${u}/${p}/${s.stream_id}.${s.container_extension || 'mp4'}` };
    return { ...base, logo: s.cover || '', seriesId: s.series_id, plot: s.plot || '' };
  });
  return { channels };
}

async function loadXtreamSeries({ server, user, pass, seriesId }) {
  const q = `username=${encodeURIComponent(user)}&password=${encodeURIComponent(pass)}`;
  const res = await fetch(`${server}/player_api.php?${q}&action=get_series_info&series_id=${seriesId}`, { headers: { 'User-Agent': UA }, signal: AbortSignal.timeout(60000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const j = await res.json();
  const u = encodeURIComponent(user), p = encodeURIComponent(pass);
  const seasons = Object.keys(j.episodes || {}).sort((a, b) => a - b).map((sn) => ({
    season: sn,
    episodes: j.episodes[sn].map((e) => ({
      title: e.title || `Episode ${e.episode_num}`, num: e.episode_num, plot: (e.info && e.info.plot) || '', duration: (e.info && e.info.duration) || '',
      url: `${server}/series/${u}/${p}/${e.id}.${e.container_extension || 'mp4'}`,
    })),
  }));
  return { seasons, plot: (j.info && j.info.plot) || '' };
}

(async () => {
  if (workerData.type === 'xtream') return parentPort.postMessage({ result: await loadXtream(workerData.source) });
  if (workerData.type === 'xtream-lib') return parentPort.postMessage({ result: await loadXtreamLib(workerData.source) });
  if (workerData.type === 'xtream-series') return parentPort.postMessage({ result: await loadXtreamSeries(workerData.source) });
  const text = await readSource(workerData.source);
  const result = workerData.type === 'm3u' ? parseM3U(text) : parseXMLTV(text);
  parentPort.postMessage({ result });
})().catch((e) => { throw e; });
