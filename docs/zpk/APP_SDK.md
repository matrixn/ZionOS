# Zion App SDK 1.0 and native desktop integration — TASK 04

[TASK 05](E2E.md) adds a real three-boot persistent integration environment, native
settings through local Unix IPC, upgrades and interrupted-operation recovery. It
does not enable browser settings/files or privileged HTTP lifecycle operations.

Installed ZPK desktop entries join the existing ZionOS launcher, dock and window
manager. The native backend stays an Alpine Linux/OpenRC process. Its UI is an
isolated browser document; no native GUI display server, runtime container or
frontend framework is required. Built-in apps and storage controls remain in the
existing static shell.

This is a versioned **local development** SDK, not an authenticated production app
platform. Zion Core remains unauthenticated, restricted to local/QEMU clients and
read-only. Package installation/service control stays in the local privileged CLI.

## Components and data flow

```text
ZPK Manager private SQLite
  -> committed enabled desktop entries
  -> root-owned /run/zionos/desktop/{registry.json,ready}
  -> unprivileged Core GET /api/apps + registered asset reads
  -> desktop Registry (builtins + installed entries)
  -> shared window chrome + sandboxed iframe
  -> Zion bridge v1 (own-window UI capabilities only)
```

- `crates/zpk-schema/src/desktop.rs` is the typed public projection v1 contract.
  `notifications` is an additive permission, and desktop entries optionally declare
  `multiple_instances = true` (default false). Existing v1 manifests still parse.
- Manager's `desktop.rs` derives the projection from validated committed records,
  without exposing SQLite, source paths, service identities, executable paths, private
  data/configuration or package-manager diagnostics to the web service.
- `core/api/src/apps.rs` reads this projection and registered immutable assets only.
  It cannot install packages or connect to private native service sockets.
- `web/desktop/apps/{registry,bridge}.js` provide the lightweight registry and bridge.
  `app.js` still owns all windows, focus, geometry, chrome and built-in apps.
- `web/zion-app-sdk/` contains TypeScript source, pinned development tools, generated
  ES-module/classic distributions and declarations. There are **no runtime dependencies**.

Manager automatically publishes after install/upgrade/rollback/uninstall. Successful
UI publication is reported as `desktop_registry: "published"`; a committed package
with a projection I/O problem reports `pending: ...`, not a false UI-publication claim.
The desktop polls every five seconds while visible and refreshes on visibility change.
An entry's revision changes when its metadata/assets change; old windows are closed
instead of retaining code from an upgraded or removed package.

Manager removes the `ready` marker before a desktop-affecting transaction, then writes
and fsyncs registry content and finally its SHA-256 marker. Core checks the marker
before/after reading and rejects mixed generations. During an interrupted GUI
activation there is no usable projection; ordinary Manager startup reconciliation
restores/commits executable state and rebuilds it. Unchanged non-desktop transactions
do not revoke existing UI views or repeatedly hash all application payloads. A clean
projection is reused. The projection is derived and volatile: after reboot, restore
persistent application mounts and run a normal Manager command such as `zpk list`.
No privileged publication daemon or boot reconciliation hook is installed here.

Local policy changes are implemented as real, persistent CLI operations:

```sh
# Prepared TARGET Alpine/ZionOS system only; does not stop/start the backend:
zpk desktop disable org.zionos.zion-hello
zpk desktop enable org.zionos.zion-hello
```

The flag survives upgrades; legacy records default enabled. The manager invalidates
publication before changing it, so failure leaves UI unavailable rather than granting
access from a stale projection. Normal CLI reconciliation retries publication.

## Read-only HTTP contract v1

`GET /api/apps` returns:

```json
{
  "schema_version": 1,
  "status": "ready",
  "revision": "<64 lowercase hex characters>",
  "authentication": {
    "mode": "local-development",
    "authenticated": false,
    "required_for_remote": true
  },
  "apps": [{
    "id": "zpk:org.zionos.zion-hello/main",
    "package_id": "org.zionos.zion-hello",
    "version": "1.0.0",
    "desktop_id": "main",
    "title": "Zion Hello",
    "categories": ["development", "utilities"],
    "window": {"width":760,"height":600,"min_width":320,"min_height":260,"resizable":true},
    "multiple_instances": true,
    "notifications": true,
    "entry_url": "/app-assets/org.zionos.zion-hello/1.0.0/ui/index.html",
    "icon_url": "/app-assets/org.zionos.zion-hello/1.0.0/assets/icon.png",
    "revision": "<entry fingerprint>"
  }]
}
```

The shell combines these entries with seven built-ins; packages cannot shadow their
IDs. A missing/invalid projection returns `status: "unavailable"`, null revision and
no installed entries. Unknown incompatible contracts are refused. The authentication
envelope explicitly reports **no authenticated principal**; it is a future integration
boundary, not an authentication implementation. LAN clients remain denied.

Only registered paths under `/app-assets/<id>/<active-version>/<path>` are served.
The server checks canonical paths, trusted ownership/modes, non-symlink ancestry,
single-link regular descriptors, size/hash, manifest fingerprint and active `current`
reference. It rechecks generation/current after reading. Executables, libraries,
manifests, checksums, LICENSE, arbitrary paths and stale versions are not served.
Errors are generic 404 responses, not filesystem paths. Supported UI resources are
HTML under `ui/` and JS/MJS, CSS and static PNG under `assets/`, each at most 1 MiB.
PNG/text content is validated before publication; every response is hash checked.

Assets have no-store, nosniff, restrictive CSP, no-referrer and hardware/clipboard
Permissions-Policy headers. Anonymous CORS is enabled **only for registered assets**
so opaque iframes can import ES modules, without credentialed CORS. The narrow
`Origin: null` exception applies only to asset GET/HEAD, never `/api/*`. POST and
package write/service routes are absent. Cookies/tokens are never serialized into
asset responses, registry entries or bridge messages.

## SDK consumption

Choose one distribution per iframe. The sample uses standard ES modules:

```js
import {connect, ZionError} from '../assets/zion-app-sdk.mjs';

const app = await connect();
await app.window.setTitle('My Zion application');
const theme = await app.theme.get();
const unsubscribe = await app.theme.subscribe(theme => renderTheme(theme));
const capabilities = await app.system.getCapabilities();
if (capabilities.methods['notifications.show']) {
  await app.notifications.show({title:'Ready', message:'Your application is ready.'});
}
```

Classic offline scripts can include `zion-app-sdk.js` and call `ZionApp.connect()`.
The ESM type entry is `dist/index.d.ts`; global-script declarations are in
`dist/zion-app-sdk.d.ts`. `tests/apps/sdk-types.ts` checks actual named imports and
consumer types. Neither distribution requires Vue, React or a runtime loader.

| API | Current enforced behavior |
|---|---|
| `connect({parentOrigin?, timeoutMs?})` | Versioned handshake; defaults to the served app URL's HTTP(S) origin. A future separate app origin must supply the pinned desktop origin explicitly. Standalone/SSR use fails typed `UNAVAILABLE`. |
| `app.window.setTitle(text)` | Changes only this instance's trusted chrome; 1..128 plain-text characters. |
| `app.window.setSize({width,height})` | Validates dimensions, respects fixed windows, clamps to app minima/viewport and returns actual size; small screens retain responsive layout. |
| `app.notifications.show({title,message})` | Desktop toast, not an OS push daemon. Requires the approved `notifications` declaration on both package and desktop entry; rate limited. |
| `app.theme.get()` / `subscribe(listener)` | Actual shell theme/accent/motion; subscribe resolves to an unsubscribe callback. |
| `app.dialog.confirm({title,message})` | Parent-owned text-only accessible modal; boolean response, cancel/abort, one global dialog, 30-second timeout. No privileged action follows confirmation. |
| `app.system.getCapabilities()` | Actual supported method flags, protocol version, `settingsPersistence:false`, `nativeBroker:false`. |
| `app.settings.get()/set()` | Typed `UNAVAILABLE`; no fake browser-local application persistence. |
| `app.files.pickDirectory()` | Typed `UNAVAILABLE`; no NAS path, device, file descriptor or directory handle. |
| `app.disconnect()` | Removes client handlers/pending requests and renews/revokes the old bridge session; reconnect is supported within the same document. |

Errors are `ZionError` with codes `UNAVAILABLE`, `DENIED`, `INVALID_ARGUMENT`,
`UNSUPPORTED_VERSION`, `INVALID_MESSAGE`, `TIMEOUT`, `CLOSED`, `BUSY`, `REPLAY` or
`UNSUPPORTED_METHOD`. Capability flags do not advertise unavailable methods as working.
RPC timeout is five seconds, or 35 seconds for confirmation. At most 32 client requests
may be pending. Caller JSON values must be bounded/cloneable.

## Bridge protocol and iframe boundary

The iframe has **only** `sandbox="allow-scripts"`: no same-origin, forms, popup,
download, top-navigation or native-device grants. The response also carries a sandbox
CSP. Parent metadata is rendered with DOM/textContent, never package-provided HTML.

Protocol v1 messages are bounded objects with exact key sets:

1. Child `hello`: `{protocol:"zion-app",version:1,type:"hello",clientNonce}`.
2. Parent `offer`: same protocol/version, instance UUID, 256-bit nonce, app ID and
   echoed client nonce. A mismatching protocol gets a typed rejection.
3. Child `ack` echoes the complete offered context plus its client nonce.
4. Parent `connected` completes the handshake. Requests before this are ignored.
5. Request: context plus `type:"request",requestId:"r1",method,params`.
6. Response: context/request ID plus `type:"response",ok,result` or typed `error`.
7. Subscribed events use `type:"event",event:"theme.changed",value`.

The parent validates `event.source === frame.contentWindow`, opaque `event.origin ===
"null"`, no transferred ports, app/instance/nonce, handshake state, registry revision/
freshness and method-specific authorization. Request numbers increase monotonically;
replay IDs are rejected without repeating effects. The bridge accepts up to 40 messages
per second per frame. Unknown methods cannot invoke system helpers. The child validates
the concrete parent source/origin, response schema and pending request ID. The parent
must target `"*"` for an opaque destination, but sends only the dedicated frame's UI
session; this is **not** an administrator token or a backend authorization credential.

A WindowProxy survives navigation. After the first iframe load, another full-document
load detaches the bridge and closes that window; the user reopens it through trusted
chrome. No new session is automatically offered to a navigated document. In-flight
responses are bound to the nonce captured before awaiting, so an old dialog cannot
resolve a newer session's request. Same-document SDK disconnect/reconnect rotates the
nonce. Uninstall/disable/revision changes close old windows on discovery; visible
polling is five seconds and bridge registry freshness expires after seven seconds.
This is bounded cached UI authorization, **not immediate server-side privilege revocation**.

CSP restricts script/style/image sources to the registered package/version prefix,
blocks fetch/XHR/WebSocket, forms, nested frames, workers, objects and base-URL changes.
This is not comprehensive network-egress or browser-resource isolation: self-navigation
can issue a request before its load is observed; browser/kernel flaws, side channels,
phishing and renderer CPU/memory abuse are not solved. No privileged broker capabilities
are available under this model. Do not claim VM-equivalent or production sandboxing.

Platform references: [iframe sandbox](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe),
[postMessage source/origin rules](https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage),
[CSP sandbox](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/sandbox).

## Native communication and future production app origins

Opening a UI does **not** invoke a privileged service operation through Core. Start
backends using Manager's local `--start`, `start` or reviewed autostart configuration.
Static UI needs no daemon; multiple windows/entries may share one backend. The sample's
native process is real and verified independently; its UI does not pretend to exchange
backend data through an unimplemented broker.

Future production design requires a dedicated app-origin service, separate from the
desktop's host-only session cookies, plus per-user/per-package authenticated discovery
and asset authorization. Keep privileged credentials in the desktop/server boundary,
not iframe JavaScript. Audit origin provisioning, TLS, redirects, navigation and CSP
before allowing less restrictive embedding or general third-party apps.

A broker must authenticate the desktop principal, bind a frame's approved package
identity to each operation, check revocable grants and audit effects. Native transport
must verify Unix peer UID/GID against the immutable registry and use typed operations,
not raw shell commands, paths, device handles or public service ports. File/settings
capabilities need opaque scoped leases and real per-user/package persistence, quotas,
revocation and migration design. A browser nonce or package-supplied ID alone is not
an authorization mechanism for that future broker. None of this production broker,
publisher signature trust or remote app catalog is implemented in TASK 04.

## Build, run and verify

From repository root in WSL/Linux:

```sh
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
npm --prefix web/zion-app-sdk run build
python3 examples/zion-hello/build-assets.py
bash tests/apps/check.sh

# First browser setup; use --with-deps on equipped CI/administrator machines as needed.
(cd web/zion-app-sdk && npx playwright install chromium)

# Native source compilation remains inside the existing constrained BUILD environment.
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build \
  --project examples/zion-hello --prepare-builder
bash tests/apps/check.sh --browser

# Final ISO includes the shell/bridge. These tests use dedicated disposable QCOW2 only.
bash scripts/build-iso.sh
bash tests/apps/check.sh --vm
```

`--browser` uses a non-root debug Core/private temporary test root and mock Manager
services. No native app content executes on the host. `--vm` additionally boots the
production Core in Alpine, installs the actual Forge package, verifies a dedicated
non-root OpenRC process, runs Chromium against forwarding bound to `127.0.0.1:8080`,
uninstalls through guest CLI and verifies asset/launcher revocation. Port 8080 must be
free; the test never terminates somebody else's VM. The image filesystem is formatted
on a newly created regular file, never a physical block device.

The prepared target requires persistent mounts and Manager/Runtime helpers as described
in [MANAGER.md](MANAGER.md). Example target install:

```sh
zpk install ./zion-hello-1.0.0.zpk --trust-local-development \
  --permissions desktop-ui,notifications,background-service,persistent-data --start --unix-health
zpk list
zpk status org.zionos.zion-hello
zpk verify org.zionos.zion-hello
```

Then open the local ZionOS desktop launcher and search for `Zion Hello`. Use the `+`
chrome button or Shift-launch for another instance, the pin button for browser-local
dock persistence, and the titlebar/resize handle with pointer or arrow keys. The normal
ISO command remains `bash scripts/run-qemu.sh`; it launches the live desktop with
built-ins only until persistent mounts/helpers/packages have been prepared. The SDK
test VM shuts down after verification and does not deploy an app to the developer host.

CI keeps all previous jobs and adds an SDK/browser job. A separate manually dispatched
VM workflow builds the final ISO and tests the native pipeline. See
[executed verification](APP_SDK_VERIFICATION.md) for results and unverified limits.
