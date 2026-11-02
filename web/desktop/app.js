'use strict';
/* ZionOS Shell Genesis 0.6. No external assets, runtime dependencies or privileged APIs. */
const APPS = [
  { id: 'overview', title: 'System Overview', short: 'Overview', icon: 'grid', tile: 'tile-activity', subtitle: 'Hardware & performance', dock: true },
  { id: 'files', title: 'File Station', short: 'Files', icon: 'folder', tile: 'tile-files', subtitle: 'File explorer · preview', dock: true },
  { id: 'storage', title: 'Storage Manager', short: 'Storage', icon: 'hard-drive', tile: 'tile-storage', subtitle: 'Devices & volumes', dock: true },
  { id: 'activity', title: 'Activity Monitor', short: 'Activity', icon: 'activity', tile: 'tile-activity', subtitle: 'Live diagnostics', dock: true },
  { id: 'network', title: 'Network', short: 'Network', icon: 'network', tile: 'tile-network', subtitle: 'Connections', dock: false },
  { id: 'settings', title: 'Settings', short: 'Settings', icon: 'settings', tile: 'tile-settings', subtitle: 'Personalize', dock: true },
  { id: 'about', title: 'About ZionOS', short: 'About', icon: 'info', tile: 'tile-about', subtitle: 'Genesis release', dock: false }
];
const byId = (id) => document.getElementById(id);
const icon = (name, size = '') => `<svg class="svg ${size}" aria-hidden="true"><use href="#i-${name}"></use></svg>`;
const appById = (id) => APPS.find((app) => app.id === id);
const windows = new Map();
const cpuHistory = [];
let zIndex = 20;
let currentMetrics = null;
let currentStorage = null;
let currentPools = null;
let poolsLoading = false;
let poolsError = null;
let plannerLayout = 'single';
const plannerSelected = new Set();
let latestPlanSummary = '';
let storageLoading = false;
let storageError = null;
let selectedStorageDisk = null;
let coreHealth = null;
let connected = false;
let polling = false;
let launcherOpen = false;
let toastTimer = null;
let prefs = { theme: 'dark', accent: 'sea', motion: 'normal' };
const prefsKey = 'zionos.genesis.0.3.preferences';

function loadPrefs() {
  try {
    const saved = JSON.parse(localStorage.getItem(prefsKey) || '{}');
    if (['dark', 'light'].includes(saved.theme)) prefs.theme = saved.theme;
    if (['sea', 'blue', 'violet'].includes(saved.accent)) prefs.accent = saved.accent;
    if (['normal', 'reduced'].includes(saved.motion)) prefs.motion = saved.motion;
  } catch { /* cookies/storage may be blocked */ }
  applyPrefs();
}
function applyPrefs() {
  document.documentElement.dataset.theme = prefs.theme;
  document.documentElement.dataset.accent = prefs.accent;
  document.documentElement.classList.toggle('reduce-motion', prefs.motion === 'reduced');
  document.querySelectorAll('[data-theme-choice]').forEach((el) => {
    el.classList.toggle('selected', el.dataset.themeChoice === prefs.theme);
    el.setAttribute('aria-pressed', String(el.dataset.themeChoice === prefs.theme));
  });
  document.querySelectorAll('[data-accent-choice]').forEach((el) => {
    el.classList.toggle('selected', el.dataset.accentChoice === prefs.accent);
    el.setAttribute('aria-pressed', String(el.dataset.accentChoice === prefs.accent));
  });
  document.querySelectorAll('[data-motion-choice]').forEach((el) => {
    el.classList.toggle('selected', el.dataset.motionChoice === prefs.motion);
    el.setAttribute('aria-pressed', String(el.dataset.motionChoice === prefs.motion));
  });
}
function savePrefs() {
  try { localStorage.setItem(prefsKey, JSON.stringify(prefs)); } catch { /* optional */ }
  applyPrefs();
}
function toast(message) {
  const el = byId('toast');
  byId('toast-text').textContent = message;
  el.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 3700);
}
function renderAppButtons() {
  const icons = byId('desktop-icons');
  const dock = byId('dock');
  const launcher = byId('launcher-grid');
  icons.replaceChildren();
  dock.replaceChildren();
  launcher.replaceChildren();
  for (const app of APPS) {
    const launch = document.createElement('button');
    launch.className = 'launcher-app';
    launch.type = 'button';
    launch.dataset.open = app.id;
    launch.innerHTML = `<span class="icon-tile ${app.tile}">${icon(app.icon)}</span><span>${app.short}</span>`;
    launcher.appendChild(launch);
    if (['overview', 'storage', 'files', 'network'].includes(app.id)) {
      const shortcut = document.createElement('button');
      shortcut.className = 'desktop-icon';
      shortcut.dataset.open = app.id;
      shortcut.title = `Deschide ${app.title}`;
      shortcut.innerHTML = `<span class="icon-tile ${app.tile}">${icon(app.icon)}</span><span>${app.short}</span>`;
      icons.appendChild(shortcut);
    }
  }
  const start = document.createElement('button');
  start.type = 'button'; start.className = 'dock-button launcher'; start.id = 'dock-launcher';
  start.title = 'Aplicații'; start.setAttribute('aria-label', 'Aplicații');
  start.innerHTML = `${icon('grid')}<span class="dock-label">Aplicații</span>`;
  start.addEventListener('click', () => toggleLauncher());
  dock.appendChild(start);
  const sep = document.createElement('span'); sep.className = 'dock-separator'; sep.setAttribute('aria-hidden', 'true');
  dock.appendChild(sep);
  for (const app of APPS.filter((a) => a.dock)) {
    const button = document.createElement('button');
    button.type = 'button'; button.className = 'dock-button'; button.dataset.dock = app.id;
    button.setAttribute('aria-label', app.title); button.title = app.title;
    button.innerHTML = `${icon(app.icon)}<span class="dock-label">${app.title}</span>`;
    button.addEventListener('click', () => {
      const win = windows.get(app.id);
      if (win && !win.classList.contains('minimized') && win.classList.contains('focused')) minimizeWindow(app.id);
      else openApp(app.id);
    });
    dock.appendChild(button);
  }
}
function updateDock() {
  document.querySelectorAll('[data-dock]').forEach((button) => {
    const win = windows.get(button.dataset.dock);
    button.classList.toggle('active', Boolean(win && !win.classList.contains('minimized')));
    button.setAttribute('aria-pressed', String(Boolean(win && !win.classList.contains('minimized'))));
  });
}
function toggleLauncher(force) {
  const wasOpen = launcherOpen;
  launcherOpen = typeof force === 'boolean' ? force : !launcherOpen;
  byId('launcher-overlay').classList.toggle('open', launcherOpen);
  byId('launcher-overlay').setAttribute('aria-hidden', String(!launcherOpen));
  if (launcherOpen) {
    byId('launcher-input').value = '';
    filterLauncher('');
    requestAnimationFrame(() => byId('launcher-input').focus());
  } else if (wasOpen) {
    byId('brand-launcher').focus();
  }
}
function filterLauncher(query) {
  const q = query.trim().toLocaleLowerCase('ro');
  document.querySelectorAll('.launcher-app').forEach((el) => {
    const app = appById(el.dataset.open);
    el.hidden = !(app && `${app.title} ${app.short} ${app.subtitle}`.toLocaleLowerCase('ro').includes(q));
    el.style.display = el.hidden ? 'none' : '';
  });
}
function focusWindow(el) {
  windows.forEach((win) => win.classList.remove('focused'));
  el.classList.remove('minimized');
  el.classList.add('focused');
  el.style.zIndex = String(++zIndex);
  updateDock();
}
function createWindow(app) {
  const win = document.createElement('section');
  win.className = 'os-window'; win.dataset.app = app.id;
  win.setAttribute('role', 'dialog'); win.setAttribute('aria-label', app.title);
  win.tabIndex = -1;
  const offset = windows.size * 28;
  win.style.left = `${Math.max(130, Math.round((innerWidth - 820) / 2)) + offset}px`;
  win.style.top = `${Math.max(86, Math.round((innerHeight - 650) / 2)) + offset}px`;
  win.innerHTML = `<div class="window-header"><span style="color:var(--accent)">${icon(app.icon, 'small')}</span><span class="window-title">${app.title}</span><span class="window-subtitle">${app.subtitle}</span><div class="window-controls"><button class="win-control" data-window-action="min" title="Minimizează" aria-label="Minimizează">${icon('minus','tiny')}</button><button class="win-control" data-window-action="max" title="Maximizează" aria-label="Maximizează">${icon('max','tiny')}</button><button class="win-control close" data-window-action="close" title="Închide" aria-label="Închide">${icon('close','tiny')}</button></div></div><div class="window-body"></div>`;
  const template = byId(`tpl-${app.id}`);
  win.querySelector('.window-body').appendChild(template.content.cloneNode(true));
  win.addEventListener('pointerdown', () => focusWindow(win));
  win.querySelectorAll('[data-window-action]').forEach((button) => button.addEventListener('click', (e) => {
    e.stopPropagation();
    if (button.dataset.windowAction === 'close') closeWindow(app.id);
    if (button.dataset.windowAction === 'min') minimizeWindow(app.id);
    if (button.dataset.windowAction === 'max') maximizeWindow(app.id);
  }));
  const header = win.querySelector('.window-header');
  header.addEventListener('dblclick', (e) => {
    if (!e.target.closest('button')) maximizeWindow(app.id);
  });
  header.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.target.closest('button') || win.classList.contains('maximized') || innerWidth < 700) return;
    const startX = e.clientX; const startY = e.clientY;
    const startLeft = win.offsetLeft; const startTop = win.offsetTop;
    function move(event) {
      const left = Math.max(0, Math.min(innerWidth - 85, startLeft + event.clientX - startX));
      const top = Math.max(54, Math.min(innerHeight - 100, startTop + event.clientY - startY));
      win.style.left = `${left}px`; win.style.top = `${top}px`;
    }
    function done() { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', done); window.removeEventListener('pointercancel', done); }
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', done);
    window.addEventListener('pointercancel', done);
  });
  windows.set(app.id, win);
  byId('window-layer').appendChild(win);
  focusWindow(win);
  return win;
}
function openApp(id) {
  const app = appById(id);
  if (!app) return;
  toggleLauncher(false);
  const win = windows.get(id) || createWindow(app);
  focusWindow(win);
  hydrateWindow(win);
  if (id === 'storage') { renderStorageWindow(win); renderPoolWindow(win); pollStorage(); pollPools(); pollLab(); }
}
function closeWindow(id) {
  const win = windows.get(id);
  if (!win) return;
  win.remove(); windows.delete(id);
  const top = Array.from(windows.values()).filter((w) => !w.classList.contains('minimized')).sort((a,b) => (+b.style.zIndex)-(+a.style.zIndex))[0];
  if (top) focusWindow(top);
  updateDock();
}
function minimizeWindow(id) {
  const win = windows.get(id);
  if (!win) return;
  win.classList.add('minimized'); win.classList.remove('focused');
  updateDock();
}
function maximizeWindow(id) {
  const win = windows.get(id);
  if (!win) return;
  win.classList.toggle('maximized'); focusWindow(win);
}
function setText(win, key, value) {
  win.querySelectorAll(`[data-bind="${key}"]`).forEach((node) => { node.textContent = String(value); });
}
function setMeter(win, key, value) {
  win.querySelectorAll(`[data-meter="${key}"]`).forEach((node) => { node.style.width = `${Math.min(100, Math.max(0, value || 0))}%`; });
}
function bytes(value) {
  const size = Math.max(0, Number(value) || 0);
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let n = size, i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(i >= 3 ? 2 : i === 0 ? 0 : 1)} ${units[i]}`;
}
function uptime(secs) {
  const v = Math.max(0, Math.floor(Number(secs) || 0));
  const d = Math.floor(v / 86400); const h = Math.floor((v % 86400) / 3600); const m = Math.floor((v % 3600) / 60);
  return d ? `${d}z ${h}h` : `${h}h ${m}m`;
}
function deviceType(disk) { return disk.removable ? 'Removable' : disk.rotational ? 'HDD' : 'SSD / virtual'; }
function makeDiskRow(disk) {
  const row = document.createElement('div'); row.className = 'disk-row';
  const badge = document.createElement('div'); badge.className = 'disk-icon'; badge.innerHTML = icon('hard-drive', 'small');
  const middle = document.createElement('div'); middle.className = 'disk-details';
  const name = document.createElement('div'); name.className = 'disk-name'; name.textContent = disk.name || 'unknown';
  const meta = document.createElement('div'); meta.className = 'disk-meta'; meta.textContent = `${disk.model || 'Dispozitiv fără model raportat'} · ${deviceType(disk)}`;
  middle.append(name, meta);
  const right = document.createElement('div'); right.className = 'disk-size'; right.textContent = bytes(disk.size_bytes);
  const state = document.createElement('div'); state.className = 'disk-state'; state.textContent = disk.read_only ? 'Kernel flag: read-only' : 'Doar detectat';
  right.appendChild(state);
  row.append(badge, middle, right);
  return row;
}
function fillDiskList(node, disks, compact) {
  if (!node) return;
  node.replaceChildren();
  if (!disks.length) {
    const text = document.createElement('div'); text.style.color = 'var(--muted)'; text.style.fontSize = '12px';
    text.textContent = 'Niciun dispozitiv bloc detectat.';
    node.appendChild(text); return;
  }
  if (compact) {
    const first = disks.slice(0, 2);
    for (const disk of first) node.appendChild(makeDiskRow(disk));
    if (disks.length > 2) { const text = document.createElement('div'); text.style.cssText = 'font-size:11px;color:var(--muted);margin-top:10px'; text.textContent = `+${disks.length - 2} alte dispozitive în Storage Manager`; node.appendChild(text); }
  } else {
    for (const disk of disks) node.appendChild(makeDiskRow(disk));
  }
}
function updateChart(win) {
  const line = win.querySelector('[data-chart="line"]');
  const area = win.querySelector('[data-chart="area"]');
  if (!line || !area) return;
  const series = cpuHistory.length ? cpuHistory : [0];
  const points = series.map((value, i) => `${(600 * i / Math.max(1, series.length - 1)).toFixed(1)},${(150 - 1.5 * value).toFixed(1)}`);
  line.setAttribute('points', points.join(' '));
  area.setAttribute('d', `M${points[0]} L${points.slice(1).join(' L')} L600 150 L0 150 Z`);
}
function hydrateWindow(win) {
  const m = currentMetrics;
  setText(win, 'health', connected ? 'Online' : 'Offline');
  win.querySelectorAll('[data-bind="led"]').forEach((el) => { el.className = `status-bead ${connected ? 'online' : 'offline'}`; });
  setText(win, 'api-version', coreHealth?.version || '—');
  setText(win, 'origin', window.location.origin);
  if (m) {
    const cpu = Number.isFinite(m.cpu_usage_percent) ? Math.min(100, Math.max(0, m.cpu_usage_percent)) : null;
    const total = Math.max(0, m.memory_total_bytes || 0);
    const available = Math.min(total, Math.max(0, m.memory_available_bytes || 0));
    const ram = total - available;
    setText(win, 'cpu', cpu === null ? '—' : `${cpu.toFixed(1)}%`);
    setText(win, 'ram', bytes(ram));
    setText(win, 'ramdetail', `${bytes(available)} disponibil · ${bytes(total)} total`);
    setText(win, 'uptime', uptime(m.uptime_seconds));
    setText(win, 'cores', `${m.cpu_cores ?? '—'} nuclee logice`);
    setText(win, 'hostname', m.hostname || 'zionos');
    setText(win, 'kernel', m.kernel || '—');
    setText(win, 'arch', m.architecture || '—');
    setText(win, 'cpumodel', m.cpu_model || '—');
    setText(win, 'diskcount', `${m.disks?.length || 0} dispozitive`);
    setMeter(win, 'cpu', cpu);
    setMeter(win, 'ram', total ? 100 * ram / total : 0);
    fillDiskList(win.querySelector('[data-bind="overview-disks"]'), Array.isArray(m.disks) ? m.disks : [], true);
    updateChart(win);
  }
  applyPrefs();
}
function updateSystemLed() {
  byId('system-led').className = `status-bead ${connected ? 'online' : 'offline'}`;
  byId('connection-label').textContent = connected ? 'System online' : 'Offline';
  windows.forEach((win) => hydrateWindow(win));
}
async function pollMetrics() {
  if (polling) return;
  polling = true;
  try {
    const [healthResponse, metricsResponse] = await Promise.all([
      fetch('/api/health', { cache: 'no-store', signal: AbortSignal.timeout(4500) }),
      fetch('/api/metrics', { cache: 'no-store', signal: AbortSignal.timeout(4500) })
    ]);
    if (!healthResponse.ok || !metricsResponse.ok) throw new Error('Zion Core indisponibil');
    coreHealth = await healthResponse.json();
    currentMetrics = await metricsResponse.json();
    connected = true;
    const cpu = currentMetrics.cpu_usage_percent;
    if (Number.isFinite(cpu)) { cpuHistory.push(Math.min(100, Math.max(0, cpu))); if (cpuHistory.length > 60) cpuHistory.shift(); }
  } catch (error) {
    connected = false;
    console.warn('ZionOS monitoring:', error.message);
  } finally {
    polling = false;
    updateSystemLed();
  }
}
function updateClock() {
  byId('clock').textContent = new Intl.DateTimeFormat('ro-RO', { hour: '2-digit', minute: '2-digit', day: 'numeric', month: 'short' }).format(new Date());
}
function wireEvents() {
  byId('brand-launcher').addEventListener('click', () => toggleLauncher());
  byId('quick-search').addEventListener('click', () => toggleLauncher(true));
  byId('theme-toggle').addEventListener('click', () => { prefs.theme = prefs.theme === 'dark' ? 'light' : 'dark'; savePrefs(); toast(`Temă ${prefs.theme === 'dark' ? 'întunecată' : 'luminoasă'} activată`); });
  byId('connection-button').addEventListener('click', () => openApp('network'));
  byId('launcher-input').addEventListener('input', (event) => filterLauncher(event.target.value));
  byId('launcher-input').addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      const first = byId('launcher-grid').querySelector('.launcher-app:not([hidden])');
      if (first) openApp(first.dataset.open);
    }
  });
  byId('launcher-overlay').addEventListener('pointerdown', (event) => {
    if (event.target === byId('launcher-overlay')) toggleLauncher(false);
  });
  document.addEventListener('click', (event) => {
    const open = event.target.closest('[data-open]');
    if (open) { openApp(open.dataset.open); return; }
    const refresh = event.target.closest('[data-refresh]');
    if (refresh) { pollMetrics(); toast('Se actualizează informațiile despre sistem.'); return; }
    const storageRefresh = event.target.closest('[data-storage-refresh]');
    if (storageRefresh) { pollStorage(); pollPools(); return; }
    if (event.target.closest('[data-pool-refresh]')) { pollPools(); return; }
    if (event.target.closest('[data-lab-refresh]')) { pollLab(); return; }
    if (event.target.closest('[data-lab-copy]')) { copyLabCommands(); return; }
    const layoutChoice = event.target.closest('[data-plan-layout]');
    if (layoutChoice) { plannerLayout = layoutChoice.dataset.planLayout; renderPoolWindow(windows.get('storage')); return; }
    if (event.target.closest('[data-plan-copy]')) { copyPlan(); return; }
    const diskButton = event.target.closest('[data-storage-disk]');
    if (diskButton) { selectedStorageDisk = diskButton.dataset.storageDisk; const win = windows.get('storage'); if (win) renderStorageWindow(win); return; }
    const theme = event.target.closest('[data-theme-choice]');
    if (theme) { prefs.theme = theme.dataset.themeChoice; savePrefs(); return; }
    const accent = event.target.closest('[data-accent-choice]');
    if (accent) { prefs.accent = accent.dataset.accentChoice; savePrefs(); return; }
    const motion = event.target.closest('[data-motion-choice]');
    if (motion) { prefs.motion = motion.dataset.motionChoice; savePrefs(); }
  });
  document.addEventListener('change', (event) => {
    const check = event.target.closest('[data-plan-disk]');
    if (!check) return;
    if (check.checked) plannerSelected.add(check.dataset.planDisk);
    else plannerSelected.delete(check.dataset.planDisk);
    renderPoolWindow(windows.get('storage'));
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && launcherOpen) { event.preventDefault(); toggleLauncher(false); }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); toggleLauncher(true); }
    if ((event.ctrlKey || event.metaKey) && event.altKey && event.key.toLowerCase() === 'o') { event.preventDefault(); openApp('overview'); }
  });
}
loadPrefs();
renderAppButtons();
wireEvents();
updateClock();
setInterval(updateClock, 30_000);
openApp('overview');
pollMetrics();
setInterval(pollMetrics, 5000);
setInterval(() => { if (windows.has('storage')) { pollStorage(); pollPools(); pollLab(); } }, 75000);


// Storage 0.4 intentionally uses DOM/textContent for all data returned by /api/storage.
// No disk-changing API or browser-side privileged function exists.
function storageElement(tag, cls, value) {
  const element = document.createElement(tag);
  if (cls) element.className = cls;
  if (value !== undefined && value !== null) element.textContent = String(value);
  return element;
}
function storagePair(label, value) {
  const row = storageElement('div', 'storage-kv');
  row.append(storageElement('span', 'k', label), storageElement('span', 'v', value ?? '—'));
  return row;
}
function storageSmartLabel(info) {
  if (!info) return { text: 'SMART indisponibil', cls: 'warn' };
  if (info.status === 'passed') return { text: 'SMART: OK', cls: 'ok' };
  if (info.status === 'failed') return { text: 'SMART: ALERTĂ', cls: 'danger' };
  if (info.status === 'standby') return { text: 'SMART: standby', cls: 'warn' };
  return { text: 'SMART: necunoscut', cls: 'warn' };
}
function storageStat(parent, label, value, description) {
  const card = storageElement('div', 'storage-total');
  card.append(storageElement('div', 'caption', label), storageElement('div', 'value', value), storageElement('div', 'sub', description));
  parent.appendChild(card);
}
function renderStorageWindow(win) {
  const totals = win.querySelector('[data-storage-totals]');
  const list = win.querySelector('[data-storage-list]');
  const detail = win.querySelector('[data-storage-detail]');
  const mountsNode = win.querySelector('[data-storage-mounts]');
  const message = win.querySelector('[data-storage-message]');
  const mountCount = win.querySelector('[data-storage-mount-count]');
  if (!totals || !list || !detail || !mountsNode) return;
  if (!currentStorage) {
    for (const node of [totals, list, detail, mountsNode]) {
      node.replaceChildren(storageElement('div', 'storage-empty', storageError || 'Se citește inventarul storage…'));
    }
    if (message) message.textContent = storageError || 'Inventar în curs de încărcare.';
    return;
  }
  const disks = Array.isArray(currentStorage.disks) ? currentStorage.disks : [];
  const mounts = Array.isArray(currentStorage.mounts) ? currentStorage.mounts : [];
  const parts = disks.flatMap((d) => Array.isArray(d.partitions) ? d.partitions : []);
  totals.replaceChildren();
  storageStat(totals, 'Discuri', disks.length, 'Dispozitive bloc fizice / logice');
  storageStat(totals, 'Capacitate detectată', bytes(disks.reduce((n, d) => n + (Number(d.size_bytes) || 0), 0)), 'Capacitate brută, poate include volume suprapuse');
  storageStat(totals, 'Partiții / mounts', `${parts.length} / ${mounts.length}`, 'Inventar în mod read-only');
  list.replaceChildren();
  if (!disks.length) list.append(storageElement('div', 'storage-empty', 'Niciun disc detectat în /sys/block.'));
  if (!disks.some((d) => d.name === selectedStorageDisk)) selectedStorageDisk = disks[0]?.name || null;
  for (const disk of disks) {
    const button = storageElement('button', `storage-device${disk.name === selectedStorageDisk ? ' selected' : ''}`);
    button.type = 'button'; button.dataset.storageDisk = disk.name;
    button.setAttribute('aria-pressed', String(disk.name === selectedStorageDisk));
    const ico = storageElement('div', 'device-icon'); ico.innerHTML = icon('hard-drive', 'small');
    const main = storageElement('div', 'device-main');
    main.append(storageElement('div', 'device-name', disk.name || 'unknown'), storageElement('div', 'device-sub', disk.model || disk.transport || 'Dispozitiv bloc'));
    button.append(ico, main, storageElement('div', 'device-size', bytes(disk.size_bytes)));
    list.appendChild(button);
  }
  detail.replaceChildren();
  const disk = disks.find((d) => d.name === selectedStorageDisk);
  if (!disk) {
    detail.append(storageElement('div', 'storage-empty', 'Selectează un disc.'));
  } else {
    const heading = storageElement('div', 'storage-detail-title');
    const left = storageElement('div');
    left.append(storageElement('h3', '', disk.name), storageElement('p', '', disk.model || disk.transport || 'Model neraportat'));
    const status = storageSmartLabel(disk.smart);
    const pill = storageElement('span', `storage-pill ${status.cls}`, status.text);
    heading.append(left, pill); detail.appendChild(heading);
    detail.append(
      storagePair('Capacitate', bytes(disk.size_bytes)),
      storagePair('Transport', disk.transport || 'Necunoscut'),
      storagePair('Tip', disk.rotational ? 'HDD' : 'SSD / virtual'),
      storagePair('Logical / physical sector', `${disk.logical_sector_bytes || '—'} / ${disk.physical_sector_bytes || '—'} B`),
      storagePair('Hardware read-only flag', disk.read_only ? 'Da' : 'Nu'),
      storagePair('Temperatură SMART', Number.isFinite(disk.smart?.temperature_c) ? `${disk.smart.temperature_c} °C` : 'Indisponibil'),
      storagePair('Ore de funcționare', disk.smart?.power_on_hours == null ? 'Indisponibil' : `${disk.smart.power_on_hours.toLocaleString('ro-RO')} h`)
    );
    if (disk.smart?.percentage_used != null) detail.append(storagePair('NVMe endurance used', `${disk.smart.percentage_used}%`));
    if (disk.smart?.reason) detail.append(storageElement('div', 'storage-footnote', disk.smart.reason));
    detail.append(storageElement('div', 'storage-part-label', 'Partiții'));
    const diskPartitions = Array.isArray(disk.partitions) ? disk.partitions : [];
    if (!diskPartitions.length) detail.append(storageElement('div', 'storage-empty', 'Nicio partiție raportată.'));
    for (const part of diskPartitions) {
      const item = storageElement('div', 'storage-part');
      const top = storageElement('div', 'storage-part-top');
      top.append(storageElement('span', '', part.name || 'partition'), storageElement('span', '', bytes(part.size_bytes)));
      const sub = [part.filesystem || 'Filesystem necunoscut', part.label ? `Etichetă: ${part.label}` : null, ...(part.mounts || []).map((m) => `Montat: ${m}`)].filter(Boolean).join(' · ');
      item.append(top, storageElement('div', 'storage-part-sub', sub)); detail.appendChild(item);
    }
  }
  if (mountCount) mountCount.textContent = `${mounts.length} mounted`;
  mountsNode.replaceChildren();
  if (!mounts.length) mountsNode.append(storageElement('div', 'storage-empty', 'Nu există volume bloc montate în spațiul de nume al procesului Zion Core.'));
  for (const mount of mounts) {
    const item = storageElement('div', 'storage-volume');
    const left = storageElement('div');
    left.append(storageElement('div', 'storage-volume-title', mount.mountpoint), storageElement('div', 'storage-volume-desc', `${mount.source} · ${mount.filesystem} · ${mount.read_only ? 'read-only mount' : 'mount rw'}`));
    const right = storageElement('div', 'storage-volume-size');
    if (mount.total_bytes != null && mount.available_bytes != null && mount.total_bytes > 0) {
      const used = Math.max(0, mount.total_bytes - mount.available_bytes);
      right.append(storageElement('div', '', `${bytes(used)} / ${bytes(mount.total_bytes)}`));
      const bar = storageElement('div', 'storage-volume-meter');
      const fill = storageElement('span'); fill.style.width = `${Math.max(0, Math.min(100, 100 * used / mount.total_bytes))}%`;
      bar.appendChild(fill); right.appendChild(bar);
    } else right.append(storageElement('div', '', 'Spațiu indisponibil'));
    item.append(left,right); mountsNode.appendChild(item);
  }
  if (message) message.textContent = [...(currentStorage.warnings || []), 'Inventar read-only. SMART poate lipsi pe discuri virtuale sau fără acces la hardware.'].join(' · ');
}
async function pollStorage() {
  if (storageLoading) return;
  storageLoading = true;
  const win = windows.get('storage');
  const refresh = win?.querySelector('[data-storage-refresh]');
  if (refresh) refresh.disabled = true;
  try {
    const response = await fetch('/api/storage', {cache: 'no-store', signal: AbortSignal.timeout(20000)});
    if (!response.ok) throw new Error(`Storage API HTTP ${response.status}`);
    const data = await response.json();
    if (!Array.isArray(data.disks) || !Array.isArray(data.mounts)) throw new Error('Format storage invalid');
    currentStorage = data;
    storageError = null;
  } catch (error) {
    storageError = `Storage Manager nu este disponibil: ${error.message}`;
    if (win) toast(storageError);
  } finally {
    storageLoading = false;
    if (refresh) refresh.disabled = false;
    const current = windows.get('storage');
    if (current) renderStorageWindow(current);
  }
}

// Genesis 0.5: observational topology and a browser-only capacity calculator.
// No privileged write endpoint exists; the UI never submits disk operations.
function poolElem(tag, cls, text) { return storageElement(tag, cls, text); }
function renderPoolWindow(win) {
  if (!win) return;
  const list = win.querySelector('[data-pool-list]');
  const volumesNode = win.querySelector('[data-pool-volumes]');
  const count = win.querySelector('[data-volume-count]');
  const disksNode = win.querySelector('[data-plan-disks]');
  const result = win.querySelector('[data-plan-result]');
  const copy = win.querySelector('[data-plan-copy]');
  if (!list || !volumesNode || !disksNode || !result) return;
  win.querySelectorAll('[data-plan-layout]').forEach(button => {
    const active = plannerLayout === button.dataset.planLayout;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  if (!currentPools) {
    const message = poolsError || 'Se citește topologia de stocare…';
    for (const node of [list, volumesNode, disksNode, result]) node.replaceChildren(poolElem('div','storage-empty',message));
    if (copy) copy.disabled = true;
    return;
  }
  const pools = Array.isArray(currentPools.pools) ? currentPools.pools : [];
  const volumes = Array.isArray(currentPools.volumes) ? currentPools.volumes : [];
  const candidates = Array.isArray(currentPools.candidates) ? currentPools.candidates : [];
  list.replaceChildren();
  if (!pools.length) list.append(poolElem('div','storage-empty','Nu au fost găsite pooluri mdRAID sau Btrfs. Este normal într-un QEMU cu disc gol.'));
  for (const pool of pools) {
    const card = poolElem('section','pool-card');
    const top = poolElem('div','pool-card-top');
    const info = poolElem('div');
    info.append(poolElem('h4','',pool.name),poolElem('p','',`${pool.kind === 'mdraid' ? 'Linux MD RAID' : 'Btrfs'} · ${pool.layout || '—'} · ${pool.evidence}`));
    const degraded = Number(pool.degraded_devices || 0) > 0;
    const status = poolElem('div',`pool-state ${degraded?'danger':''}`, degraded ? `${pool.degraded_devices} disc(uri) degradate` : (pool.state || 'detectat'));
    top.append(info,status); card.append(top);
    const row = poolElem('div','pool-meta');
    row.append(poolElem('span','pool-chip',`Membri: ${pool.members?.join(', ') || 'neraportați'}`));
    row.append(poolElem('span','pool-chip',`Dimensiune bloc: ${pool.capacity_bytes == null ? 'indisponibilă' : bytes(pool.capacity_bytes)}`));
    if (pool.sync_action) row.append(poolElem('span','pool-chip',`Sincronizare: ${pool.sync_action}`));
    card.append(row); list.append(card);
  }
  if (count) count.textContent = `${volumes.length} inventariate`;
  volumesNode.replaceChildren();
  if (!volumes.length) volumesNode.append(poolElem('div','storage-empty','Nu sunt volume bloc sau partiții identificabile. Sistemul live Alpine poate utiliza doar RAM.'));
  for (const volume of volumes) {
    const card = poolElem('div','pool-volume-card');
    const top = poolElem('div','pool-volume-top');
    const info = poolElem('div');
    info.append(poolElem('h4','',volume.label || volume.device),
      poolElem('p','',`${volume.filesystem || 'Filesystem neidentificat'} · ${volume.mountpoints?.join(', ') || 'Nemontat'} · ${volume.pool_id || 'Fără pool identificat'}`));
    const usage = volume.size_bytes == null ? 'Capacitate indisponibilă' : bytes(volume.size_bytes);
    top.append(info,poolElem('span','pool-chip',usage)); card.append(top);
    if (volume.capacity_basis === 'filesystem' && volume.size_bytes > 0 && volume.available_bytes != null) {
      const used = Math.min(100,Math.max(0,100*(volume.size_bytes-volume.available_bytes)/volume.size_bytes));
      const bar = poolElem('div','pool-volume-usage');
      const fill = poolElem('span'); fill.style.width = `${used}%`; bar.append(fill); card.append(bar);
      card.append(poolElem('p','',`${bytes(volume.available_bytes)} disponibil · ${used.toFixed(1)}% ocupat`));
    } else {
      card.append(poolElem('p','',volume.capacity_basis === 'partition' ? 'Mărime de partiție, spațiul liber nu poate fi dedus.' : 'Capacitate liberă necunoscută.'));
    }
    volumesNode.append(card);
  }
  disksNode.replaceChildren();
  const valid = new Set(candidates.map(d=>d.name));
  for (const selection of Array.from(plannerSelected)) if (!valid.has(selection)) plannerSelected.delete(selection);
  if (!candidates.length) disksNode.append(poolElem('div','storage-empty','Niciun dispozitiv bloc pentru simulare.'));
  for (const disk of candidates) {
    const row = poolElem('label','pool-check');
    const input = document.createElement('input'); input.type='checkbox'; input.dataset.planDisk = disk.name;
    input.checked = plannerSelected.has(disk.name); input.setAttribute('aria-label',`Include ${disk.name} în simulare`);
    const desc = poolElem('span'); desc.append(poolElem('strong','',`${disk.name} · ${bytes(disk.size_bytes)}`),poolElem('small','',disk.warning || disk.model || 'Dispozitiv detectat'));
    row.append(input,desc);disksNode.append(row);
  }
  const selected = candidates.filter(d=>plannerSelected.has(d.name));
  const countRequired = plannerLayout === 'single' ? 1 : plannerLayout === 'mirror' ? 2 : 3;
  const validLayout = plannerLayout === 'single' ? selected.length === 1 : selected.length >= countRequired;
  latestPlanSummary = '';
  result.replaceChildren();
  if (!validLayout) {
    result.append(poolElem('div','',plannerLayout==='single' ? 'Single necesită exact un disc.' : `Selectează cel puțin ${countRequired} discuri.`));
    if (copy) copy.disabled=true;
    return;
  }
  const sizes = selected.map(d=>Number(d.size_bytes) || 0);
  const min = Math.min(...sizes);
  const estimated = plannerLayout === 'single' ? min : plannerLayout === 'mirror' ? min : min * (sizes.length - 1);
  const redundancy = plannerLayout === 'single' ? 'Fără redundanță' : plannerLayout === 'mirror' ? 'Copii complete; toleranța la defect depinde de implementare' : 'Paritate simplă; poate tolera pierderea unui disc';
  latestPlanSummary = `ZionOS Genesis 0.5 — SIMULARE FĂRĂ EXECUȚIE\nLayout: ${plannerLayout}\nDiscuri: ${selected.map(d=>d.name).join(', ')}\nCapacitate teoretică: ${bytes(estimated)}\n${redundancy}\nNu se creează/șterge/formatează nimic. Datele existente nu sunt verificate.`;
  result.append(poolElem('strong','',bytes(estimated)),poolElem('div','',`Capacitate utilă teoretică · ${redundancy}`),poolElem('div','storage-footnote','Nu include metadata, filesystem, rezervă, aliniere sau riscurile configurării.'));
  if (copy) copy.disabled=false;
}
async function copyPlan() {
  if (!latestPlanSummary) return;
  try { await navigator.clipboard.writeText(latestPlanSummary); toast('Simularea a fost copiată.'); }
  catch { toast('Clipboard indisponibil; folosește HTTPS sau localhost.'); }
}
async function pollPools() {
  if (poolsLoading) return;
  poolsLoading=true;
  const win = windows.get('storage');
  const refresh=win?.querySelector('[data-pool-refresh]');
  if (refresh) refresh.disabled=true;
  try {
    const r = await fetch('/api/pools', {cache:'no-store',signal:AbortSignal.timeout(25000)});
    if (!r.ok) throw new Error(`Pools API HTTP ${r.status}`);
    const d = await r.json();
    if (!Array.isArray(d.pools) || !Array.isArray(d.volumes) || !Array.isArray(d.candidates) || d.mode !== 'read-only') throw new Error('Schema /api/pools invalidă');
    currentPools=d; poolsError=null;
  } catch(e) { poolsError=`Pools indisponibile: ${e.message}`; currentPools=null; }
  finally { poolsLoading=false;if(refresh)refresh.disabled=false;renderPoolWindow(windows.get('storage')); }
}


// Genesis 0.6: purely observational lab diagnostics. There are NO write calls from web.
let currentLab = null;
const labCommandText = 'zion-lab-provision --plan\nzion-lab-recover --check';
async function pollLab() {
  try {
    const response = await fetch('/api/provisioning', {cache:'no-store',signal:AbortSignal.timeout(10000)});
    if (!response.ok) throw new Error('HTTP ' + response.status);
    const data = await response.json();
    if (data.mode !== 'read-only' || !Array.isArray(data.reasons) || !data.recovery || data.recovery.automatic_repairs !== false)
      throw new Error('Schema provisioning invalidă');
    currentLab = data;
    renderLabWindow(windows.get('storage'));
  } catch(error) {
    currentLab = null;
    const node = windows.get('storage')?.querySelector('[data-lab-reasons]');
    if (node) node.replaceChildren(storageElement('div','storage-empty',`Provisioning indisponibil: ${error.message}`));
  }
}
function renderLabWindow(win) {
  if (!win || !currentLab) return;
  const totals = win.querySelector('[data-lab-totals]');
  const reasons = win.querySelector('[data-lab-reasons]');
  const commands = win.querySelector('[data-lab-commands]');
  const recovery = win.querySelector('[data-lab-recovery]');
  if (!totals || !reasons || !commands || !recovery) return;
  totals.replaceChildren();
  storageStat(totals, 'Mediu', currentLab.qemu_detected ? 'QEMU' : 'Alt hardware', 'Operațiile lab sunt restricționate la QEMU');
  storageStat(totals, 'Discul vdb', currentLab.target_exists ? 'Detectat' : 'Absent', currentLab.target_capacity_bytes ? bytes(currentLab.target_capacity_bytes) : 'Disc separat lipsă');
  storageStat(totals, 'Preflight', currentLab.eligible_for_lab ? 'Parțial OK' : 'Blocat', 'CLI verifică suplimentar signatures și mount-uri');
  reasons.replaceChildren();
  for (const message of currentLab.reasons) reasons.append(storageElement('div','storage-footnote',message));
  commands.textContent = 'WSL: bash scripts/run-qemu-lab.sh\nConsola QEMU: zion-lab-provision --plan\nConsola QEMU: zion-lab-recover --check\n\nDoar pentru un disc QCOW2 nou și separat. Formatarea necesită comanda manuală --apply și o confirmare în consola QEMU.';
  recovery.replaceChildren();
  recovery.append(storageElement('h4','', 'Recovery diagnostics (fără modificări)'));
  for (const step of currentLab.recovery.steps || []) recovery.append(storageElement('p','',`• ${step}`));
  for (const limitation of currentLab.recovery.limitations || []) recovery.append(storageElement('div','storage-footnote',limitation));
}
async function copyLabCommands() {
  try { await navigator.clipboard.writeText(labCommandText); toast('Comenzile read-only au fost copiate.'); }
  catch { toast('Clipboard indisponibil.'); }
}
