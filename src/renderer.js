'use strict';
const $ = (id) => document.getElementById(id);
const ROW_H = 60;
const LS = {
  get: (k, d) => { try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; } },
  set: (k, v) => localStorage.setItem(k, JSON.stringify(v)),
};

const state = {
  channels: [], groups: [], epg: { programs: {}, names: {} },
  view: [], group: '*all', query: '', current: null, kind: 'live', libs: {}, series: null,
  favs: new Set(LS.get('favs', [])), recent: LS.get('recent', []),
  src: LS.get('src', { playlist: '', epg: '' }),
};
const favKey = (c) => c.url || 's' + c.seriesId;
const norm = (s) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');

/* ---------- EPG helpers ---------- */
function progsFor(c) {
  if (c._pr !== undefined && c._prEpg === state.epg) return c._pr;
  const { programs, names } = state.epg;
  let p = c.epgId && programs[c.epgId];
  if (!p) { const id = names[c._n || (c._n = norm(c.name))]; p = id && programs[id]; }
  c._pr = p || null; c._prEpg = state.epg;
  return c._pr;
}
function nowNext(c, t = Date.now()) {
  const p = progsFor(c);
  if (!p) return null;
  for (let i = 0; i < p.length; i++) if (p[i][1] > t) return p[i][0] <= t ? { now: p[i], next: p[i + 1] } : { now: null, next: p[i] };
  return null;
}
const fmt = (t) => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

/* ---------- Groups ---------- */
function buildGroups() {
  const counts = new Map();
  for (const c of state.channels) counts.set(c.group, (counts.get(c.group) || 0) + 1);
  state.groups = [...counts].sort((a, b) => a[0].localeCompare(b[0]));
  renderGroups();
}
function renderGroups() {
  const items = [
    ['*all', 'All channels', state.channels.length, 'sep'],
    ['*fav', '★ Favorites', state.favs.size, ''],
    ['*recent', 'Recent', state.recent.length, 'sep'],
    ...state.groups.map(([g, n]) => [g, g, n, '']),
  ];
  const frag = document.createDocumentFragment();
  for (const [key, label, n, cls] of items) {
    const d = document.createElement('div');
    d.className = 'grp ' + cls + (key === state.group ? ' active' : '');
    d.dataset.key = key;
    d.innerHTML = '<span></span><span class="n"></span>';
    d.children[0].textContent = label; d.children[1].textContent = n;
    frag.appendChild(d);
  }
  $('groups').replaceChildren(frag);
}
$('groups').addEventListener('click', (e) => {
  const g = e.target.closest('.grp'); if (!g) return;
  state.group = g.dataset.key; renderGroups(); applyFilter(true);
});

/* ---------- Filtering ---------- */
let searchTimer;
$('search').addEventListener('input', () => { clearTimeout(searchTimer); searchTimer = setTimeout(() => { state.query = $('search').value.trim().toLowerCase(); applyFilter(true); }, 120); });

function applyFilter(reset) {
  const { group, query } = state;
  let src = state.channels;
  if (group === '*fav') src = src.filter((c) => state.favs.has(favKey(c)));
  else if (group === '*recent') { const m = new Map(state.channels.map((c) => [c.url, c])); src = state.recent.map((u) => m.get(u)).filter(Boolean); }
  else if (group !== '*all') src = src.filter((c) => c.group === group);
  if (query) {
    const terms = query.split(/\s+/);
    src = src.filter((c) => { const l = c._l || (c._l = c.name.toLowerCase()); return terms.every((t) => l.includes(t)); });
  }
  state.view = src;
  $('listMeta').textContent = `${src.length.toLocaleString()} channels`;
  $('spacer').style.height = src.length * ROW_H + 'px';
  if (reset) $('list').scrollTop = 0;
  renderList(true);
}

/* ---------- Virtual list ---------- */
const list = $('list');
const pool = new Map(); // index -> element
let raf = 0;
list.addEventListener('scroll', () => { if (!raf) raf = requestAnimationFrame(() => { raf = 0; renderList(); }); }, { passive: true });
new ResizeObserver(() => renderList()).observe(list);

function renderList(force) {
  const top = list.scrollTop, h = list.clientHeight;
  const from = Math.max(0, Math.floor(top / ROW_H) - 6);
  const to = Math.min(state.view.length, Math.ceil((top + h) / ROW_H) + 6);
  if (force) { for (const el of pool.values()) el.remove(); pool.clear(); }
  for (const [i, el] of pool) if (i < from || i >= to) { el.remove(); pool.delete(i); }
  const frag = document.createDocumentFragment();
  for (let i = from; i < to; i++) {
    if (pool.has(i)) continue;
    const el = makeRow(state.view[i], i);
    pool.set(i, el); frag.appendChild(el);
  }
  list.appendChild(frag);
}
function makeRow(c, i) {
  const el = document.createElement('div');
  el.className = 'row-ch' + (c === state.current ? ' active' : '');
  el.style.top = i * ROW_H + 'px';
  el.dataset.i = i;
  const lg = document.createElement('div'); lg.className = 'lg';
  if (c.logo && /^https?:/i.test(c.logo)) {
    const img = new Image(); img.loading = 'lazy'; img.decoding = 'async'; img.src = c.logo;
    img.onerror = () => { img.remove(); lg.textContent = c.name[0] || '?'; };
    lg.appendChild(img);
  } else lg.textContent = c.name[0] || '?';
  const tx = document.createElement('div'); tx.className = 'tx';
  const nm = document.createElement('div'); nm.className = 'nm'; nm.textContent = c.name;
  tx.appendChild(nm);
  const nn = nowNext(c);
  if (nn && nn.now) {
    const pg = document.createElement('div'); pg.className = 'pg'; pg.textContent = nn.now[2];
    const bar = document.createElement('div'); bar.className = 'bar';
    const pct = Math.min(100, ((Date.now() - nn.now[0]) / (nn.now[1] - nn.now[0])) * 100);
    bar.innerHTML = `<i style="width:${pct}%"></i>`;
    tx.append(pg, bar);
  } else if (c.kind && c.kind !== 'live') {
    const pg = document.createElement('div'); pg.className = 'pg';
    pg.textContent = c.group + (c.rating && +c.rating ? '  ·  ★ ' + c.rating : '');
    tx.appendChild(pg);
  }
  el.append(lg, tx);
  if (state.favs.has(favKey(c))) { const s = document.createElement('div'); s.className = 'st'; s.textContent = '★'; el.appendChild(s); }
  return el;
}
list.addEventListener('click', (e) => {
  const r = e.target.closest('.row-ch'); if (!r) return;
  const c = state.view[+r.dataset.i];
  if (c.seriesId) openSeries(c); else play(c);
});

/* ---------- Player ---------- */
const video = $('video');
let hls = null, mpegts = null, loadToken = 0;

function destroyPlayers() {
  if (hls) { hls.destroy(); hls = null; }
  if (mpegts) { try { mpegts.destroy(); } catch {} mpegts = null; }
  video.removeAttribute('src'); video.load();
}
function setStatus(msg) { const s = $('status'); s.textContent = msg || ''; s.classList.toggle('hidden', !msg); }

function startHls(url, token) {
  hls = new Hls({ enableWorker: true, lowLatencyMode: false, maxBufferLength: 20, backBufferLength: 30, manifestLoadingMaxRetry: 2 });
  hls.loadSource(url); hls.attachMedia(video);
  hls.on(Hls.Events.FRAG_LOADED, () => token === loadToken && setStatus(''));
  hls.on(Hls.Events.ERROR, (_, d) => {
    if (token !== loadToken || !d.fatal) return;
    if (d.type === Hls.ErrorTypes.NETWORK_ERROR && !hls._retried) { hls._retried = true; hls.startLoad(); }
    else if (d.type === Hls.ErrorTypes.MEDIA_ERROR) hls.recoverMediaError();
    else if (!playFallbackTs(hls.url || url, token)) setStatus('Stream unavailable');
  });
}
function startTs(url, token, retried) {
  mpegts = mpegts_.createPlayer({ type: 'mpegts', isLive: true, url }, { enableWorker: true, liveBufferLatencyChasing: true, liveBufferLatencyMaxLatency: 6, liveBufferLatencyMinRemain: 1, autoCleanupSourceBuffer: true });
  mpegts.attachMediaElement(video); mpegts.load();
  mpegts.on(mpegts_.Events.ERROR, () => {
    if (token !== loadToken) return;
    destroyPlayers();
    if (!retried) setTimeout(() => token === loadToken && (startTs(url, token, true), video.play().catch(() => {})), 400);
    else startHls(url, token); // some "extensionless" URLs are really HLS
  });
}
const mpegts_ = window.mpegts;

function playFallbackTs(url, token) {
  if (/\/live\/.+\.m3u8$/.test(url) && mpegts_ && mpegts_.isSupported()) { destroyPlayers(); startTs(url.replace(/\.m3u8$/, '.ts'), token); return true; }
  return false;
}
async function loadXtreamEpg(c) {
  const s = state.src;
  if (!c.sid || !s.server || c._xe || c.kind !== 'live') return;
  c._xe = true;
  try {
    const u = `${s.server}/player_api.php?username=${encodeURIComponent(s.user)}&password=${encodeURIComponent(s.pass)}&action=get_short_epg&stream_id=${c.sid}&limit=20`;
    const j = await (await fetch(u)).json();
    const b64 = (x) => { try { return new TextDecoder().decode(Uint8Array.from(atob(x || ''), (ch) => ch.charCodeAt(0))); } catch { return x || ''; } };
    const p = (j.epg_listings || []).map((e) => [e.start_timestamp * 1000, e.stop_timestamp * 1000, b64(e.title), b64(e.description).slice(0, 600)]).sort((a, b) => a[0] - b[0]);
    if (p.length) { c._pr = p; c._prEpg = state.epg; if (state.current === c) renderInfo(); renderList(true); }
  } catch { c._xe = false; }
}

function play(c) {
  if (!c) return;
  const token = ++loadToken;
  state.current = c;
  if (!c.ep) state.series = null;
  destroyPlayers();
  $('empty').classList.add('hidden');
  setStatus('Loading…');
  const url = c.url;
  if (/\.m3u8?(\?|$)|m3u8/i.test(url) && Hls.isSupported()) startHls(url, token);
  else if (/\.(mp4|webm|ogg|mp3|aac|mkv|m4v|mov)(\?|$)/i.test(url)) video.src = url;
  else if (mpegts_ && mpegts_.isSupported()) startTs(url, token);
  else video.src = url;
  video.play().catch(() => {});
  setTimeout(() => { if (token === loadToken && video.readyState < 3) setStatus('Stream not responding — the channel may be offline'); }, 15000);
  state.recent = [url, ...state.recent.filter((u) => u !== url)].slice(0, 30);
  LS.set('recent', state.recent); LS.set('last', url);
  $('osdTitle').textContent = c.name;
  loadXtreamEpg(c); updateFavBtn(); renderInfo(); markActive();
}
function updateRes() {
  const w = video.videoWidth, h = video.videoHeight, el = $('osdRes');
  if (!w || !h || !state.current) { el.classList.add('hidden'); return; }
  const q = h >= 2000 ? '4K' : h >= 1400 ? '2K' : h >= 1000 ? 'FHD' : h >= 700 ? 'HD' : 'SD';
  el.textContent = `${q} · ${w}×${h}`; el.classList.remove('hidden');
}
video.addEventListener('loadedmetadata', updateRes); video.addEventListener('resize', updateRes);
video.addEventListener('emptied', updateRes);
video.addEventListener('playing', () => setStatus(''));
video.addEventListener('waiting', () => state.current && setStatus('Buffering…'));
video.addEventListener('error', () => state.current && setStatus('Cannot play this stream'));
video.addEventListener('play', () => $('cPlay').textContent = '❚❚');
video.addEventListener('pause', () => $('cPlay').textContent = '▶');

function markActive() {
  for (const [i, el] of pool) el.classList.toggle('active', state.view[i] === state.current);
}
function step(d) {
  const eps = state.series && state.series.flat;
  if (eps && eps.includes(state.current)) {
    const j = eps.indexOf(state.current) + d;
    if (j >= 0 && j < eps.length) playEpisode(eps[j]);
    return;
  }
  if (!state.view.length) return;
  let i = state.view.indexOf(state.current);
  i = i < 0 ? 0 : (i + d + state.view.length) % state.view.length;
  play(state.view[i]);
  const y = i * ROW_H;
  if (y < list.scrollTop || y > list.scrollTop + list.clientHeight - ROW_H) list.scrollTop = Math.max(0, y - list.clientHeight / 2);
}

/* ---------- Info / EPG panel ---------- */
function renderInfo() {
  if (state.series) return renderSeriesPanel();
  const c = state.current; if (!c) return;
  const head = $('infoHead'), epgEl = $('epgList');
  if (c.kind && c.kind !== 'live') {
    head.innerHTML = '<h1></h1><div class="sub"></div>';
    head.children[0].textContent = c.name; head.children[1].textContent = c.group;
    epgEl.replaceChildren();
    return;
  }
  const nn = nowNext(c);
  head.innerHTML = '<h1></h1><div class="sub"></div>';
  head.children[0].textContent = c.name; head.children[1].textContent = c.group;
  $('osdNow').textContent = nn && nn.now ? `${nn.now[2]}  ·  ${fmt(nn.now[0])} – ${fmt(nn.now[1])}` : '';
  const p = progsFor(c);
  if (!p) { epgEl.innerHTML = '<div class="prog"><div></div><div class="ds">No program guide for this channel.</div></div>'; return; }
  const t = Date.now(), frag = document.createDocumentFragment();
  for (const pr of p.filter((x) => x[1] > t).slice(0, 40)) {
    const live = pr[0] <= t;
    const d = document.createElement('div'); d.className = 'prog' + (live ? ' now' : '');
    const a = document.createElement('div'); a.className = 't'; a.textContent = `${fmt(pr[0])} – ${fmt(pr[1])}`;
    const b = document.createElement('div');
    const ti = document.createElement('div'); ti.className = 'ti'; ti.textContent = pr[2];
    b.appendChild(ti);
    if (pr[3]) { const ds = document.createElement('div'); ds.className = 'ds'; ds.textContent = pr[3]; b.appendChild(ds); }
    if (live) b.insertAdjacentHTML('beforeend', `<div class="bar"><i style="width:${Math.min(100, ((t - pr[0]) / (pr[1] - pr[0])) * 100)}%"></i></div>`);
    d.append(a, b); frag.appendChild(d);
  }
  epgEl.replaceChildren(frag);
}
setInterval(() => { if (state.current) renderInfo(); renderList(true); }, 60000);

/* ---------- Controls ---------- */
function updateFavBtn() { $('cFav').textContent = state.current && state.favs.has(favKey(state.current)) ? '★' : '☆'; }
function toggleFav() {
  const c = state.current; if (!c) return;
  const k = favKey(c);
  state.favs.has(k) ? state.favs.delete(k) : state.favs.add(k);
  LS.set('favs', [...state.favs]); updateFavBtn(); renderGroups();
  if (state.group === '*fav') applyFilter(false); else renderList(true);
}
const player = $('player');
$('cPlay').onclick = () => (video.paused ? video.play() : video.pause());
$('cPrev').onclick = () => step(-1);
$('cNext').onclick = () => step(1);
$('cFav').onclick = toggleFav;
$('cMute').onclick = () => { video.muted = !video.muted; $('cMute').textContent = video.muted ? '🔇' : '🔊'; };
$('cVol').oninput = (e) => { video.volume = +e.target.value; video.muted = false; LS.set('vol', video.volume); };
$('cVol').value = video.volume = LS.get('vol', 1);
$('cPip').onclick = () => (document.pictureInPictureElement ? document.exitPictureInPicture() : video.requestPictureInPicture().catch(() => {}));
const toggleFull = () => (document.fullscreenElement ? document.exitFullscreen() : player.requestFullscreen());
$('cFull').onclick = toggleFull;
video.addEventListener('dblclick', toggleFull);
video.addEventListener('click', () => (video.paused ? video.play() : video.pause()));

let uiTimer;
const showUi = () => { player.classList.add('ui'); clearTimeout(uiTimer); uiTimer = setTimeout(() => !video.paused && player.classList.remove('ui'), 2800); };
player.addEventListener('mousemove', showUi); player.addEventListener('mouseleave', () => player.classList.remove('ui'));

window.addEventListener('keydown', (e) => {
  const typing = e.target.tagName === 'INPUT';
  if (e.key === 'Escape') { if (typing) e.target.blur(); closeModal(); return; }
  if (e.key === '/' && !typing) { e.preventDefault(); $('search').focus(); return; }
  if (typing) return;
  switch (e.key) {
    case ' ': e.preventDefault(); video.paused ? video.play() : video.pause(); break;
    case 'ArrowUp': case 'PageUp': e.preventDefault(); step(-1); break;
    case 'ArrowDown': case 'PageDown': e.preventDefault(); step(1); break;
    case 'ArrowLeft': case 'ArrowRight': {
      const dir = e.key === 'ArrowLeft' ? -1 : 1;
      if (isFinite(video.duration)) video.currentTime = Math.min(video.duration, Math.max(0, video.currentTime + dir * 10));
      else { video.volume = Math.min(1, Math.max(0, video.volume + dir * 0.05)); $('cVol').value = video.volume; }
      break;
    }
    case 'Enter': toggleFull(); break;
    case 'f': case 'F': toggleFav(); break;
    case 'm': case 'M': $('cMute').click(); break;
  }
});

/* ---------- Sources / loading ---------- */
function closeModal() { $('modal').classList.add('hidden'); }
let mode = 'x';
function setMode(m) {
  mode = m;
  $('boxX').classList.toggle('hidden', m !== 'x'); $('boxM').classList.toggle('hidden', m !== 'm');
  $('tabX').classList.toggle('primary', m === 'x'); $('tabM').classList.toggle('primary', m === 'm');
}
$('tabX').onclick = () => setMode('x'); $('tabM').onclick = () => setMode('m');
$('btnSettings').onclick = () => {
  const s = state.src;
  $('inPlaylist').value = s.playlist || ''; $('inEpg').value = s.epg || '';
  $('inServer').value = s.server || ''; $('inUser').value = s.user || ''; $('inPass').value = s.pass || '';
  setMode(s.playlist && !s.server ? 'm' : 'x');
  $('modalMsg').textContent = ''; $('modal').classList.remove('hidden');
};
$('mClose').onclick = closeModal;
$('browsePl').onclick = async () => { const p = await api.openFile(); if (p) $('inPlaylist').value = p; };
$('browseEpg').onclick = async () => { const p = await api.openFile(); if (p) $('inEpg').value = p; };
$('mLoad').onclick = async () => {
  const msg = $('modalMsg'); msg.className = 'msg';
  let pl, ep, creds = {};
  if (mode === 'x') {
    let server = $('inServer').value.trim().replace(/\/+$/, '');
    const user = $('inUser').value.trim(), pass = $('inPass').value;
    if (server && !/^https?:\/\//i.test(server)) server = 'http://' + server;
    if (!server || !user || !pass) { msg.textContent = 'Enter server, username and password.'; msg.classList.add('err'); return; }
    pl = { server, user, pass }; ep = '';
    creds = { server, user, pass };
  } else {
    pl = $('inPlaylist').value.trim(); ep = $('inEpg').value.trim();
    if (!pl) { msg.textContent = 'Enter a playlist URL or file.'; msg.classList.add('err'); return; }
  }
  $('mLoad').disabled = true;
  try {
    msg.textContent = 'Loading playlist…';
    const r = await api.loadPlaylist(pl);
    state.src = { playlist: typeof pl === 'string' ? pl : '', epg: ep, ...creds }; LS.set('src', state.src);
    if (r.base) { state.src.server = r.base; LS.set('src', state.src); }
    setChannels(r.channels);
    const epgUrl = ep || r.epgUrl;
    if (epgUrl) {
      msg.textContent = `${r.channels.length.toLocaleString()} channels. Loading guide…`;
      try { setEpg(await api.loadEpg(epgUrl)); } catch (e) { msg.textContent = 'Playlist loaded; guide failed: ' + e.message; msg.classList.add('err'); $('mLoad').disabled = false; return; }
    }
    closeModal();
  } catch (e) { msg.textContent = 'Failed: ' + e.message; msg.classList.add('err'); }
  $('mLoad').disabled = false;
};

function setChannels(ch) {
  state.libs = { live: ch, movie: null, series: null }; state.kind = 'live'; state.series = null;
  state.channels = ch; state.group = '*all'; state.current = state.current && ch.find((c) => c.url === state.current.url) || null;
  updateKinds(); buildGroups(); applyFilter(true);
}

/* ---------- Live / Movies / Series (Xtream) ---------- */
const xtCreds = () => (state.src.server && state.src.user ? { server: state.src.server, user: state.src.user, pass: state.src.pass } : null);
function updateKinds() {
  $('kinds').classList.toggle('hidden', !xtCreds());
  for (const b of $('kinds').children) b.classList.toggle('active', b.dataset.k === state.kind);
}
$('kinds').addEventListener('click', async (e) => {
  const k = e.target.dataset && e.target.dataset.k; if (!k || k === state.kind) return;
  const creds = xtCreds(); if (!creds) return;
  const token = (state.kindToken = (state.kindToken || 0) + 1);
  state.kind = k; state.series = null; updateKinds();
  if (!state.libs[k]) {
    $('listMeta').textContent = 'Loading…'; $('spacer').style.height = '0px'; renderList(true);
    try { state.libs[k] = (await api.loadLib({ ...creds, kind: k })).channels; }
    catch (err) { if (token === state.kindToken) { $('listMeta').textContent = 'Failed to load: ' + err.message; state.kind = 'live'; updateKinds(); } return; }
  }
  if (token !== state.kindToken) return;
  state.channels = state.libs[k]; state.group = '*all'; state.query = ''; $('search').value = '';
  buildGroups(); applyFilter(true);
  if (state.current) renderInfo();
});

async function openSeries(c) {
  const creds = xtCreds(); if (!creds) return;
  $('infoHead').innerHTML = '<h1></h1><div class="sub">Loading episodes…</div>';
  $('infoHead').children[0].textContent = c.name; $('epgList').replaceChildren();
  state.series = { item: c, seasons: null, flat: [], plot: c.plot };
  const mine = state.series;
  try {
    const r = await api.loadSeries({ ...creds, seriesId: c.seriesId });
    if (state.series !== mine) return;
    mine.seasons = r.seasons; mine.plot = r.plot || c.plot;
    for (const s of r.seasons) for (const ep of s.episodes) {
      ep.kind = 'series'; ep.ep = true; ep.group = c.group; ep.name = `${c.name} · S${s.season}E${ep.num} · ${ep.title}`; mine.flat.push(ep);
    }
    renderSeriesPanel();
  } catch (e) { if (state.series === mine) $('infoHead').children[1].textContent = 'Failed to load: ' + e.message; }
}
function renderSeriesPanel() {
  const s = state.series; if (!s || !s.seasons) return;
  const head = $('infoHead');
  head.innerHTML = '<h1></h1><div class="sub"></div>';
  head.children[0].textContent = s.item.name; head.children[1].textContent = s.plot || s.item.group;
  const frag = document.createDocumentFragment();
  s.seasons.forEach((se, si) => {
    const h = document.createElement('div'); h.className = 'season'; h.textContent = 'Season ' + se.season; frag.appendChild(h);
    se.episodes.forEach((ep, ei) => {
      const d = document.createElement('div'); d.className = 'prog ep' + (ep === state.current ? ' now' : '');
      d.dataset.s = si; d.dataset.e = ei;
      const a = document.createElement('div'); a.className = 't'; a.textContent = `E${ep.num}` + (ep.duration ? '  ' + ep.duration : '');
      const b = document.createElement('div');
      const ti = document.createElement('div'); ti.className = 'ti'; ti.textContent = ep.title; b.appendChild(ti);
      if (ep.plot) { const ds = document.createElement('div'); ds.className = 'ds'; ds.textContent = ep.plot; b.appendChild(ds); }
      d.append(a, b); frag.appendChild(d);
    });
  });
  $('epgList').replaceChildren(frag);
}
$('epgList').addEventListener('click', (e) => {
  const d = e.target.closest('.prog.ep'); if (!d || !state.series) return;
  playEpisode(state.series.seasons[+d.dataset.s].episodes[+d.dataset.e]);
});
function playEpisode(ep) { play(ep); }

/* ---------- Seek bar (on-demand only) ---------- */
const hms = (t) => { t = Math.max(0, Math.floor(t)); const h = Math.floor(t / 3600), m = Math.floor(t / 60) % 60, s = t % 60; return (h ? h + ':' + String(m).padStart(2, '0') : m) + ':' + String(s).padStart(2, '0'); };
let seeking = false;
function updateSeek() {
  const vod = isFinite(video.duration) && video.duration > 0;
  $('cSeek').classList.toggle('hidden', !vod); $('cTime').classList.toggle('hidden', !vod);
  if (!vod) return;
  if (!seeking) $('cSeek').value = (video.currentTime / video.duration) * 1000;
  $('cTime').textContent = `${hms(video.currentTime)} / ${hms(video.duration)}`;
}
video.addEventListener('timeupdate', updateSeek); video.addEventListener('durationchange', updateSeek);
$('cSeek').addEventListener('input', () => { seeking = true; video.currentTime = ($('cSeek').value / 1000) * video.duration; });
$('cSeek').addEventListener('change', () => { seeking = false; });
video.addEventListener('ended', () => { if (state.series && state.series.flat.includes(state.current)) step(1); });
function setEpg(e) {
  state.epg = e;
  if (state.current) renderInfo();
  renderList(true);
}

(async function boot() {
  $('empty').classList.remove('hidden');
  const [pl, ep] = await Promise.all([api.readCache('playlist'), api.readCache('epg')]);
  if (!pl) { $('btnSettings').click(); return; }
  setChannels(pl.channels);
  if (ep) setEpg(ep);
  const last = LS.get('last', null);
  const lc = last && state.channels.find((c) => c.url === last);
  if (lc) { state.current = lc; $('osdTitle').textContent = lc.name; renderInfo(); markActive(); }
  // Refresh stale data in the background
  const stale = (x) => !x || Date.now() - x.ts > 12 * 3600e3;
  try {
    const src = state.src;
    const xt = src.server && src.user ? { server: src.server, user: src.user, pass: src.pass } : null;
    if (stale(pl) && (xt || src.playlist)) { const r = await api.loadPlaylist(xt || src.playlist); setChannels(r.channels); pl.epgUrl = r.epgUrl; }
    const epgUrl = state.src.epg || pl.epgUrl;
    if (epgUrl && stale(ep)) setEpg(await api.loadEpg(epgUrl));
  } catch (e) { console.warn('Background refresh failed', e); }
})();
