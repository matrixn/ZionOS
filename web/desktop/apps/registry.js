/* Read-only registry adapter. No package HTML is interpolated into desktop markup. */
(function(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.ZionDesktopRegistry = api;
})(typeof window === 'undefined' ? globalThis : window, function() {
  'use strict';
  const object = v => v !== null && typeof v === 'object' && !Array.isArray(v);
  const text = (v,n) => typeof v === 'string' && v.trim().length > 0 && v.length <= n && !/[\u0000-\u001f\u007f-\u009f]/.test(v);
  const id = v => typeof v === 'string' && v.length <= 253 && v.split('.').length >= 3 && v.split('.').every(p => /^[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p));
  const version = v => typeof v === 'string' && v.length <= 240 && /^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?(?:\+[a-zA-Z0-9.-]+)?$/.test(v);
  const path = v => typeof v === 'string' && v.length <= 240 && v.split('/').every(p => p !== '.' && p !== '..' && /^[a-zA-Z0-9._+-]{1,100}$/.test(p));
  const categories = ['utilities','development','multimedia','network','storage','backup','productivity','system'];
  function entry(e) {
    if (!object(e) || !id(e.package_id) || !version(e.version) || !text(e.desktop_id,64) || !/^[a-z][a-z0-9-]*$/.test(e.desktop_id) ||
      e.id !== `zpk:${e.package_id}/${e.desktop_id}` || !text(e.title,128) || !Array.isArray(e.categories) || !e.categories.length || e.categories.length > 16 || e.categories.some(c => !categories.includes(c)) ||
      typeof e.notifications !== 'boolean' || typeof e.multiple_instances !== 'boolean' || !/^[a-f0-9]{64}$/.test(e.revision) || !object(e.window)) throw new Error('Invalid installed desktop entry');
    const w = e.window;
    if (!['width','height','min_width','min_height'].every(k => Number.isInteger(w[k])) || typeof w.resizable !== 'boolean' ||
      w.min_width < 160 || w.min_width > 7680 || w.min_height < 120 || w.min_height > 4320 || w.width < w.min_width || w.width > 7680 || w.height < w.min_height || w.height > 4320) throw new Error('Invalid installed window');
    const prefix = `/app-assets/${e.package_id}/${e.version}/`;
    if (typeof e.entry_url !== 'string' || typeof e.icon_url !== 'string' || !e.entry_url.startsWith(prefix) || !e.icon_url.startsWith(prefix)) throw new Error('Foreign app URL');
    const ui = e.entry_url.slice(prefix.length), icon = e.icon_url.slice(prefix.length);
    if (!path(ui) || !ui.startsWith('ui/') || !ui.endsWith('.html') || !path(icon) || !icon.startsWith('assets/') || !icon.endsWith('.png')) throw new Error('Unsafe app asset URL');
    return Object.freeze({ id:e.id, packageId:e.package_id, desktopId:e.desktop_id, title:e.title, short:e.title, subtitle:`Installed app · ${e.version}`, categories:[...e.categories],
      native:true, tile:'tile-native', iconURL:e.icon_url, entryURL:e.entry_url, revision:e.revision, window:{...w}, multipleInstances:e.multiple_instances, notifications:e.notifications, dock:false });
  }
  class Registry {
    constructor(builtins) { this.builtins = new Map(builtins.map(a => [a.id,Object.freeze({...a,native:false})])); this.installed = new Map(); }
    all() { return [...this.builtins.values(),...this.installed.values()]; }
    get(id) { return this.builtins.get(id) || this.installed.get(id); }
    replace(snapshot) {
      if (!object(snapshot) || snapshot.schema_version !== 1 || !['ready','unavailable'].includes(snapshot.status) || !Array.isArray(snapshot.apps) || snapshot.apps.length > 256 ||
        !object(snapshot.authentication) || snapshot.authentication.mode !== 'local-development' || snapshot.authentication.authenticated !== false || snapshot.authentication.required_for_remote !== true ||
        (snapshot.status === 'ready' ? !/^[a-f0-9]{64}$/.test(snapshot.revision) : snapshot.revision !== null || snapshot.apps.length)) throw new Error('Unsupported app registry contract');
      const next = new Map();
      for (const raw of snapshot.apps) { const a = entry(raw); if (next.has(a.id) || this.builtins.has(a.id)) throw new Error('App ID collision'); next.set(a.id,a); }
      const removed = [...this.installed.keys()].filter(id => !next.has(id));
      const added = [...next.keys()].filter(id => !this.installed.has(id));
      const changed = [...next.values()].filter(a => this.installed.has(a.id) && this.installed.get(a.id).revision !== a.revision).map(a => a.id);
      this.installed = next;
      return {removed,changed,added};
    }
    clear() { const removed = [...this.installed.keys()]; this.installed.clear(); return {removed,changed:[],added:[]}; }
  }
  return {Registry,validateEntry:entry};
});
