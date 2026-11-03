'use strict';
// Observe a REAL guest CLI upgrade/uninstall while an installed app window stays open.
const assert = require('node:assert/strict');
const path = require('node:path');
const {chromium} = require(path.resolve(__dirname, '../../web/zion-app-sdk/node_modules/playwright'));
const origin = 'http://127.0.0.1:8080';
const id = 'zpk:org.zionos.zion-hello/main';
const [before, after] = process.argv.slice(2);
assert.ok(['1.0.0', '1.1.0'].includes(before));
assert.ok(['1.1.0', 'removed'].includes(after));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function wait(check, message) {
  const end = Date.now() + 40000;
  while (Date.now() < end) { if (await check()) return; await delay(100); }
  throw new Error(message);
}
async function main() {
  const browser = await chromium.launch({headless:true});
  try {
    const page = await browser.newPage();
    await page.goto(origin);
    const app = page.locator(`.launcher-app[data-open="${id}"]`);
    const windows = page.locator(`.os-window[data-app="${id}"]`);
    await wait(() => app.count(), 'Installed app missing');
    await page.locator('#brand-launcher').click(); await app.click();
    await wait(async () => {
      const frame = page.frames().find(f => f.url().includes(`/${before}/ui/`));
      return frame && await frame.locator('#connection').textContent() === 'Connected to ZionOS';
    }, 'Old-version SDK did not connect');
    console.log('ZION_E2E_BROWSER_READY');
    await wait(async () => {
      const state = await (await page.request.get(`${origin}/api/apps`)).json();
      return after === 'removed' ? state.status === 'ready' && state.apps.length === 0 : state.apps[0]?.version === after;
    }, 'CLI change was not published');
    await wait(async () => await windows.count() === 0, 'Old window survived registry revocation');
    assert.equal((await page.request.get(`${origin}/app-assets/org.zionos.zion-hello/${before}/ui/index.html`)).status(),404);
    if (after === 'removed') {
      assert.equal(await app.count(),0);
    } else {
      await wait(() => app.count(), 'New version missing from launcher');
      await page.locator('#brand-launcher').click(); await app.click();
      await wait(async () => {
        const frame = page.frames().find(f => f.url().includes(`/${after}/ui/`));
        return frame && await frame.locator('#connection').textContent() === 'Connected to ZionOS';
      }, 'Upgraded window did not connect');
      assert.equal(await windows.locator('.window-title').textContent(),'Zion Hello · Your space');
    }
    console.log(`PASS live desktop ${before} -> ${after}: old window/asset revoked, launcher updated`);
  } finally { await browser.close(); }
}
main().catch(error => { console.error(error); process.exitCode=1; });
