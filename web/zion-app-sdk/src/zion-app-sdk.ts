/** Zion App SDK 1.0 — global namespace for classic offline scripts. Bridge protocol v1. */
namespace ZionApp {
  export const protocolVersion = 1;
  export type ErrorCode = 'UNAVAILABLE' | 'DENIED' | 'INVALID_ARGUMENT' | 'UNSUPPORTED_VERSION' |
    'INVALID_MESSAGE' | 'TIMEOUT' | 'CLOSED' | 'BUSY' | 'REPLAY' | 'UNSUPPORTED_METHOD';
  export class ZionError extends Error {
    readonly code: ErrorCode;
    constructor(code: ErrorCode, message: string) { super(message); this.name = 'ZionError'; this.code = code; }
  }
  export interface Theme { mode: 'dark' | 'light'; accent: 'sea' | 'blue' | 'violet'; motion: 'normal' | 'reduced' }
  export interface Capabilities {
    protocol: 1;
    methods: Record<string, boolean>;
    settingsPersistence: false;
    nativeBroker: false;
    isolation: 'sandboxed-opaque-iframe';
  }
  export interface Size { width: number; height: number }
  export interface App {
    readonly id: string;
    readonly instanceId: string;
    window: { setTitle(title: string): Promise<void>; setSize(size: Size): Promise<Size> };
    notifications: { show(options: { title: string; message: string }): Promise<{ shown: true }> };
    theme: { get(): Promise<Theme>; subscribe(listener: (theme: Theme) => void): Promise<() => void> };
    dialog: { confirm(options: { title: string; message: string }): Promise<boolean> };
    /** Explicitly unavailable in v1: no pretend browser-local persistent settings. */
    settings: { get(key: string): Promise<never>; set(key: string, value: unknown): Promise<never> };
    /** No NAS path, file descriptor or directory handle is exposed in v1. */
    files: { pickDirectory(): Promise<never> };
    system: { getCapabilities(): Promise<Capabilities> };
    disconnect(): void;
  }
  export interface ConnectOptions { parentOrigin?: string; timeoutMs?: number }
  type Dict = Record<string, unknown>;
  interface Pending { resolve(value: unknown): void; reject(error: ZionError): void; timer: ReturnType<typeof setTimeout> }
  function object(v: unknown): v is Dict { return v !== null && typeof v === 'object' && !Array.isArray(v); }
  function keys(v: Dict, expected: string[]): boolean { return Object.keys(v).sort().join(',') === expected.sort().join(','); }
  function theme(v: unknown): v is Theme {
    return object(v) && keys(v, ['mode','accent','motion']) && ['dark','light'].includes(String(v.mode)) &&
      ['sea','blue','violet'].includes(String(v.accent)) && ['normal','reduced'].includes(String(v.motion));
  }
  const codes: ErrorCode[] = ['UNAVAILABLE','DENIED','INVALID_ARGUMENT','UNSUPPORTED_VERSION','INVALID_MESSAGE','TIMEOUT','CLOSED','BUSY','REPLAY','UNSUPPORTED_METHOD'];
  let connection: Promise<App> | undefined;
  export function connect(options: ConnectOptions = {}): Promise<App> {
    if (connection) return connection;
    connection = establish(options).catch(error => { connection = undefined; throw error; });
    return connection;
  }
  function establish(options: ConnectOptions): Promise<App> {
    return new Promise((resolve, reject) => {
      if (typeof window === 'undefined' || window.parent === window) { reject(new ZionError('UNAVAILABLE','Open this application inside ZionOS Desktop.')); return; }
      let origin: string;
      try {
        origin = options.parentOrigin ?? new URL(window.location.href).origin;
        if (!/^https?:$/.test(new URL(origin).protocol) || new URL(origin).origin !== origin) throw new Error('origin');
      } catch { reject(new ZionError('INVALID_ARGUMENT','A concrete HTTP(S) parent origin is required.')); return; }
      const timeout = options.timeoutMs ?? 5000;
      if (!Number.isInteger(timeout) || timeout < 500 || timeout > 15000) { reject(new ZionError('INVALID_ARGUMENT','Handshake timeout must be 500..15000 ms.')); return; }
      const clientNonce = Array.from(crypto.getRandomValues(new Uint8Array(16)), b => b.toString(16).padStart(2,'0')).join('');
      let session: { instanceId: string; nonce: string; appId: string } | undefined;
      let ready = false, closed = false, counter = 0;
      const pending = new Map<string, Pending>();
      const listeners = new Set<(value: Theme) => void>();
      const send = (value: Dict) => window.parent.postMessage({ protocol: 'zion-app', version: 1, ...value }, origin);
      let retry: ReturnType<typeof setInterval> | undefined;
      let handshake: ReturnType<typeof setTimeout> | undefined;
      const clearHandshake = () => { if (retry) clearInterval(retry); if (handshake) clearTimeout(handshake); };
      const dispose = () => {
        if (closed) return;
        if (session) { try { send({type:'disconnect',...session}); } catch { /* unloading frame */ } }
        closed = true; clearHandshake(); window.removeEventListener('message', receive); window.removeEventListener('pagehide', dispose);
        for (const p of pending.values()) { clearTimeout(p.timer); p.reject(new ZionError('CLOSED','Application bridge closed.')); }
        pending.clear(); listeners.clear(); connection = undefined;
      };
      const request = <T>(method: string, params: Dict): Promise<T> => {
        if (!ready || !session || closed) return Promise.reject(new ZionError('CLOSED','Application bridge is not connected.'));
        if (pending.size >= 32) return Promise.reject(new ZionError('BUSY','Too many pending SDK requests.'));
        if (counter >= 2147483647) return Promise.reject(new ZionError('CLOSED','Reload this app to renew its session.'));
        try { if (JSON.stringify(params).length > 8192) throw new Error('size'); }
        catch { return Promise.reject(new ZionError('INVALID_ARGUMENT','Parameters must be bounded JSON values.')); }
        const requestId = `r${++counter}`;
        return new Promise<T>((ok, fail) => {
          const timer = setTimeout(() => { pending.delete(requestId); fail(new ZionError('TIMEOUT',`SDK request timed out: ${method}`)); }, method === 'dialog.confirm' ? 35000 : 5000);
          pending.set(requestId, { resolve: value => ok(value as T), reject: fail, timer });
          try { send({ type: 'request', ...session, requestId, method, params }); }
          catch { clearTimeout(timer); pending.delete(requestId); fail(new ZionError('INVALID_ARGUMENT','Parameters cannot be cloned safely.')); }
        });
      };
      function receive(event: MessageEvent) {
        if (closed || event.source !== window.parent || event.origin !== origin || event.ports.length !== 0 || !object(event.data)) return;
        const m = event.data;
        try { if (JSON.stringify(m).length > 16384) return; } catch { return; }
        if (m.protocol !== 'zion-app') return;
        if (m.version !== 1) { if (!ready) { clearHandshake(); dispose(); reject(new ZionError('UNSUPPORTED_VERSION','Unsupported Zion bridge protocol.')); } return; }
        if (m.type === 'rejected' && !ready && m.clientNonce === clientNonce && object(m.error) && m.error.code === 'UNSUPPORTED_VERSION' && typeof m.error.message === 'string' && m.error.message.length <= 512) { dispose(); reject(new ZionError('UNSUPPORTED_VERSION',m.error.message)); return; }
        if (m.type === 'offer' && !ready && m.clientNonce === clientNonce &&
          keys(m,['protocol','version','type','clientNonce','instanceId','nonce','appId']) &&
          typeof m.instanceId === 'string' && /^[a-f0-9-]{36}$/.test(m.instanceId) && typeof m.nonce === 'string' && /^[a-f0-9]{64}$/.test(m.nonce) && typeof m.appId === 'string' && m.appId.length <= 400) {
          session = { instanceId: m.instanceId, nonce: m.nonce, appId: m.appId };
          send({ type: 'ack', ...session, clientNonce }); return;
        }
        if (!session || m.instanceId !== session.instanceId || m.nonce !== session.nonce || m.appId !== session.appId) return;
        if (m.type === 'connected' && !ready && keys(m,['protocol','version','type','instanceId','nonce','appId'])) {
          ready = true; clearHandshake();
          const app: App = {
            id: session.appId, instanceId: session.instanceId,
            window: { setTitle: async title => { await request('window.setTitle',{title}); }, setSize: size => request<Size>('window.setSize',{...size}) },
            notifications: { show: value => request('notifications.show',{...value}) },
            theme: {
              get: async () => { const value = await request<unknown>('theme.get',{}); if (!theme(value)) throw new ZionError('INVALID_MESSAGE','Invalid theme response.'); return value; },
              subscribe: async listener => {
                if (typeof listener !== 'function') throw new ZionError('INVALID_ARGUMENT','Theme subscriber must be a function.');
                const value = await request<unknown>('theme.subscribe',{});
                if (!theme(value)) throw new ZionError('INVALID_MESSAGE','Invalid subscription theme.');
                listeners.add(listener); listener(value);
                return () => { listeners.delete(listener); if (!listeners.size) void request('theme.unsubscribe',{}).catch(() => {}); };
              }
            },
            dialog: { confirm: value => request('dialog.confirm',{...value}) },
            settings: { get: key => request<never>('settings.get',{key}), set: (key,value) => request<never>('settings.set',{key,value}) },
            files: { pickDirectory: () => request<never>('files.pickDirectory',{}) },
            system: { getCapabilities: () => request<Capabilities>('system.getCapabilities',{}) },
            disconnect: dispose
          };
          resolve(app); return;
        }
        if (!ready) return;
        if (m.type === 'event' && m.event === 'theme.changed' && keys(m,['protocol','version','type','instanceId','nonce','appId','event','value']) && theme(m.value)) {
          for (const listener of listeners) { try { listener(m.value); } catch { /* subscriber errors do not break the transport */ } } return;
        }
        if (m.type !== 'response' || typeof m.requestId !== 'string' || typeof m.ok !== 'boolean' ||
          !keys(m,['protocol','version','type','instanceId','nonce','appId','requestId','ok',m.ok ? 'result' : 'error'])) return;
        const p = pending.get(m.requestId); if (!p) return;
        if (!m.ok && (!object(m.error) || !keys(m.error,['code','message']) || !codes.includes(m.error.code as ErrorCode) || typeof m.error.message !== 'string' || m.error.message.length > 512)) return;
        clearTimeout(p.timer); pending.delete(m.requestId);
        if (m.ok) p.resolve(m.result);
        else { const error = m.error as Dict; p.reject(new ZionError(error.code as ErrorCode,error.message as string)); }
      }
      const hello = () => send({type:'hello',clientNonce});
      window.addEventListener('message',receive); window.addEventListener('pagehide',dispose);
      handshake = setTimeout(() => { dispose(); reject(new ZionError('TIMEOUT','ZionOS bridge handshake timed out.')); },timeout);
      retry = setInterval(hello,100); hello();
    });
  }
}
