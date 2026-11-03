# Zion App SDK 1.0

Framework-independent TypeScript browser SDK for the versioned ZionOS iframe bridge.
No runtime dependencies or CDN. ESM and classic-script distributions, declarations
and an actual TypeScript import consumer are included.

```js
import {connect} from './zion-app-sdk.mjs';
const app = await connect();
await app.window.setTitle('My application');
const theme = await app.theme.get();
```

Classic scripts can use `ZionApp.connect()` after loading `zion-app-sdk.js`.
Notifications require approved package AND entry permission. Settings/directory access
are typed `UNAVAILABLE`, not fake persistent APIs. No native service broker is exposed.

```sh
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
npm --prefix web/zion-app-sdk run build
python3 examples/zion-hello/build-assets.py
bash tests/apps/check.sh
bash tests/apps/check.sh --browser
```

The generated distributions are deliberately tracked so packages work offline; tests
compare them byte-for-byte with TypeScript output and verify the example copy.
The development Playwright/TypeScript dependencies are never shipped into the ISO/app.

Full API, wire protocol, asset/permission boundary, native backend and future dedicated
origin design: [APP_SDK.md](../../docs/zpk/APP_SDK.md).
