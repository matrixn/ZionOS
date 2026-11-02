'use strict';
// Owned application only. Local mode uses a mock installer/private temporary root;
// VM mode talks to an already installed native fixture through localhost forwarding.
const assert=require('node:assert/strict');
const fs=require('node:fs');const os=require('node:os');const path=require('node:path');const net=require('node:net');const {spawn,execFileSync}=require('node:child_process');
const ROOT=path.resolve(__dirname,'../..');
const {chromium}=require(path.join(ROOT,'web/zion-app-sdk/node_modules/playwright'));
const ID='zpk:org.zionos.zion-hello/main',PACKAGE='org.zionos.zion-hello';
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function wait(fn,message,timeout=12000){let error;const until=Date.now()+timeout;while(Date.now()<until){try{if(await fn())return;}catch(e){error=e;}await delay(70);}throw new Error(`${message}${error?`: ${error.message}`:''}`);}
async function freePort(){const server=net.createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port;await new Promise(resolve=>server.close(resolve));return port;}
async function main(){
  let root,server,output='',browser;const external=process.env.ZION_SDK_ORIGIN;
  const removed=process.argv.includes('--removed');
  const core=process.env.ZION_CORE_BIN;
  const fixture=process.env.ZION_SDK_FIXTURE_BIN;
  const archive=process.env.ZION_SDK_PACKAGE||path.join(ROOT,'build/zpk/zion-hello-sdk-1.0.0.zpk');
  const fixtureCall=(command,operand)=>{execFileSync(fixture,[root,command,operand],{cwd:ROOT,stdio:'pipe'});};
  try{
    const origin=external||`http://127.0.0.1:${await freePort()}`;
    if(!external){
      assert.ok(core&&fixture,'Set ZION_CORE_BIN and ZION_SDK_FIXTURE_BIN to the debug binaries');
      root=fs.mkdtempSync(path.join(os.tmpdir(),'zion-sdk-'));fs.chmodSync(root,0o700);
      server=spawn(core,[],{cwd:ROOT,env:{...process.env,ZION_PORT:new URL(origin).port,ZION_WEB_DIR:path.join(ROOT,'web/desktop'),ZION_APPS_TEST_ROOT:root},stdio:['ignore','pipe','pipe']});
      server.stdout.on('data',b=>{output+=b;});server.stderr.on('data',b=>{output+=b;});
    }
    await wait(async()=>{try{return (await fetch(`${origin}/api/health`)).ok;}catch{return false;}},'Core did not start');
    browser=await chromium.launch({headless:true});
    const context=await browser.newContext({viewport:{width:1280,height:900}});const page=await context.newPage();
    const messages=[];page.on('console',m=>{if(m.type()==='error')messages.push(m.text());});page.on('pageerror',e=>messages.push(e.message));
    // Avoid host storage probes in browser fixtures. Native VM health/registry/assets
    // remain real; no browser route is used for package metadata or app UI assets.
    await page.route('**/api/metrics',route=>route.fulfill({json:{cpu_usage_percent:12,memory_total_bytes:1073741824,memory_available_bytes:536870912,disks:[],hostname:'browser-fixture',architecture:'x86_64',kernel:'fixture',uptime_seconds:60,cpu_cores:2}}));
    await page.route('**/api/storage',route=>route.fulfill({json:{disks:[],summary:{},warnings:[]}}));
    await page.route('**/api/pools',route=>route.fulfill({json:{mode:'read-only',pools:[],volumes:[],candidates:[]}}));
    await page.route('**/api/provisioning',route=>route.fulfill({json:{mode:'read-only',execution_enabled:false,reasons:[],recovery:{automatic_repairs:false}}}));
    await page.goto(origin);await wait(()=>page.locator('.os-window[data-app="overview"]').count(),'Built-in overview missing');
    assert.equal(await page.locator('.launcher-app').count(),external&&!removed?8:7);
    if(removed){
      assert.equal(await page.locator(`.launcher-app[data-open="${ID}"]`).count(),0);
      assert.equal((await page.request.get(`${origin}/app-assets/${PACKAGE}/1.0.0/ui/index.html`)).status(),404);
      console.log('PASS browser VM uninstall discovery and asset revocation');return;
    }
    if(!external){
      assert.equal((await (await fetch(`${origin}/api/apps`)).json()).apps.length,0);
      fixtureCall('install',archive);
    }
    await wait(()=>page.locator(`.launcher-app[data-open="${ID}"]`).count(),'Installed app was not discovered automatically');
    const registry=await (await fetch(`${origin}/api/apps`)).json();assert.equal(registry.authentication.authenticated,false);assert.equal(registry.authentication.required_for_remote,true);assert.equal(registry.apps[0].id,ID);
    const launcher=page.locator('#launcher-input');await page.locator('#brand-launcher').click();await launcher.fill('development');
    assert.equal(await page.locator(`.launcher-app[data-open="${ID}"]`).isVisible(),true);
    await page.locator(`.launcher-app[data-open="${ID}"]`).click();
    const windows=()=>page.locator(`.os-window[data-app="${ID}"]`);
    await wait(async()=>{const frames=page.frames().filter(f=>f.url().includes('/app-assets/'));return frames[0]&&await frames[0].locator('#connection').textContent()==='Connected to ZionOS';},`SDK handshake failed; browser errors: ${messages.join('; ')}`);
    let frame=page.frames().find(f=>f.url().includes('/app-assets/'));
    assert.equal(await windows().first().locator('.window-title').textContent(),'Zion Hello · Your space');
    assert.equal(await windows().first().locator('iframe').getAttribute('sandbox'),'allow-scripts');
    assert.equal(await windows().first().locator('.window-app-icon img').evaluate(img=>img.naturalWidth),64);
    const isolation=await frame.evaluate(()=>{
      const result={};for(const [key,fn]of [['parent',()=>parent.document.body],['cookie',()=>document.cookie],['storage',()=>localStorage.length]]){try{fn();result[key]=false;}catch{result[key]=true;}}
      return result;
    });assert.deepEqual(isolation,{parent:true,cookie:true,storage:true});
    assert.equal(await frame.evaluate(async()=>{try{await fetch('/api/metrics');return false;}catch{return true;}}),true);
    assert.equal(await frame.evaluate(()=>window.open('https://example.com')===null),true);
    const asset=await page.request.get(`${origin}/app-assets/${PACKAGE}/1.0.0/ui/index.html`);
    assert.ok(asset.headers()['content-security-policy'].includes("sandbox allow-scripts"));assert.ok(!asset.headers()['content-security-policy'].includes('allow-same-origin'));
    for(const suffix of ['payload/bin/zion-hello','manifest.toml','LICENSE','assets/absent.js','%2e%2e%2fetc%2fpasswd','%2fetc%2fpasswd'])assert.notEqual((await page.request.get(`${origin}/app-assets/${PACKAGE}/1.0.0/${suffix}`)).status(),200,suffix);
    assert.equal((await page.request.post(`${origin}/api/apps`)).status(),405);
    assert.equal((await page.request.get(`${origin}/api/apps`,{headers:{Origin:'null'}})).status(),403);
    assert.equal((await page.request.get(`${origin}/app-assets/${PACKAGE}/1.0.0/assets/hello.js`,{headers:{Origin:'null'}})).status(),200);
    await frame.locator('#notify').click();await wait(async()=> (await page.locator('#toast-text').textContent()).includes('Your native application'),'Notification was not shown by the desktop');
    await page.locator('#theme-toggle').click();await wait(()=>frame.locator('html').getAttribute('data-theme').then(v=>v==='light'),'Theme subscription did not update app');
    await frame.locator('#directory').click();await wait(async()=> (await frame.locator('#result').textContent()).includes('UNAVAILABLE: Directory'),'Unavailable directory capability was not typed');
    await frame.locator('#settings').click();await wait(async()=> (await frame.locator('#result').textContent()).includes('UNAVAILABLE: Persistent application settings'),'Unavailable settings capability was not typed');
    await frame.locator('#capabilities').click();await wait(async()=> (await frame.locator('#result').textContent()).startsWith('{'),'Capabilities missing');
    const capabilities=JSON.parse(await frame.locator('#result').textContent());assert.equal(capabilities.methods['notifications.show'],true);assert.equal(capabilities.methods['settings.set'],false);assert.equal(capabilities.nativeBroker,false);
    await frame.locator('#confirm').click();await page.locator('.app-confirm button').last().click();await wait(async()=> (await frame.locator('#result').textContent())==='Confirmed: true','Confirmation bridge failed');
    await frame.locator('#resize').click();await wait(async()=> (await windows().first().boundingBox()).width===820,'SDK window size was not applied');
    const header=windows().first().locator('.window-header');await header.focus();const before=await windows().first().boundingBox();await page.keyboard.press('ArrowRight');const moved=await windows().first().boundingBox();assert.ok(moved.x>before.x);
    const box=await header.boundingBox();await page.mouse.move(box.x+90,box.y+24);await page.mouse.down();await page.mouse.move(box.x+130,box.y+44,{steps:5});await page.mouse.up();assert.ok((await windows().first().boundingBox()).x>=moved.x+25);
    const resize=windows().first().locator('.window-resize');await resize.focus();const size=await windows().first().boundingBox();await page.keyboard.press('ArrowRight');assert.ok((await windows().first().boundingBox()).width>size.width);
    await windows().first().locator('[data-window-action="max"]').click();assert.equal(await windows().first().evaluate(w=>w.classList.contains('maximized')),true);assert.ok((await windows().first().boundingBox()).width>=1278);
    await windows().first().locator('[data-window-action="max"]').click();
    await windows().first().locator('[data-window-action="min"]').click();assert.equal(await windows().first().isVisible(),false);await page.locator(`[data-dock="${ID}"]`).click();assert.equal(await windows().first().isVisible(),true);
    await windows().first().locator('[aria-label="Fereastră nouă"]').click();await wait(()=>windows().count().then(n=>n===2),'Multiple native windows missing');
    await wait(async()=>{const fs=page.frames().filter(f=>f.url().includes('/app-assets/'));return fs.length===2&&(await Promise.all(fs.map(f=>f.locator('#connection').textContent()))).every(t=>t==='Connected to ZionOS');},'Second SDK instance failed');
    const sessions=await page.evaluate(()=>[...appBridge.contexts.values()].map(c=>({nonce:c.nonce,instanceId:c.instanceId,appId:c.app.id,last:c.last})));
    assert.notEqual(sessions[0].instanceId,sessions[1].instanceId);
    const second=page.frames().filter(f=>f.url().includes('/app-assets/'))[1];
    // Even a known sibling nonce cannot overcome event.source binding.
    await second.evaluate(s=>parent.postMessage({protocol:'zion-app',version:1,type:'request',nonce:s.nonce,instanceId:s.instanceId,appId:s.appId,requestId:`r${s.last+1}`,method:'window.setTitle',params:{title:'FORGED SIBLING TITLE'}},new URL(location.href).origin),sessions[0]);
    await delay(150);assert.equal(await windows().first().locator('.window-title').textContent(),'Zion Hello · Your space');
    await windows().last().locator('[data-window-action="pin"]').click();assert.equal(await windows().last().locator('[data-window-action="pin"]').getAttribute('aria-pressed'),'true');
    await page.screenshot({path:path.join(ROOT,'build/sdk-desktop.png'),fullPage:true});
    await page.reload();await wait(()=>page.locator(`[data-dock="${ID}"]`).count(),'Dock pin did not persist in browser preferences');assert.equal(await windows().count(),0);
    await page.locator(`[data-dock="${ID}"]`).click();await wait(()=>windows().count(),'Pinned app did not reopen');
    await page.setViewportSize({width:375,height:760});await wait(async()=> (await windows().first().boundingBox()).width<=376,'Mobile window overflows viewport');assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
    await page.screenshot({path:path.join(ROOT,'build/sdk-desktop-mobile.png'),fullPage:true});
    await page.setViewportSize({width:1280,height:900});await page.locator('#brand-launcher').focus();await page.keyboard.press('Control+k');await launcher.fill('storage');await launcher.press('Enter');assert.equal(await page.locator('.os-window[data-app="storage"]').isVisible(),true);
    frame=page.frames().find(f=>f.url().includes('/app-assets/'));await frame.evaluate(()=>{location.href='about:blank';});await wait(()=>windows().count().then(n=>n===0),'Self-navigation did not revoke the app window');
    if(!external){
      fixtureCall('disable',PACKAGE);await wait(()=>page.locator(`.launcher-app[data-open="${ID}"]`).count().then(n=>n===0),'Disabled app still in launcher');assert.equal(await windows().count(),0);
      fixtureCall('enable',PACKAGE);await wait(()=>page.locator(`.launcher-app[data-open="${ID}"]`).count(),'Enabled app missing');
      await page.locator('#brand-launcher').click();await launcher.fill('Zion Hello');await launcher.press('Enter');await wait(()=>windows().count(),'App did not reopen after enable');
      fixtureCall('uninstall',PACKAGE);await wait(()=>page.locator(`.launcher-app[data-open="${ID}"]`).count().then(n=>n===0),'Removed app still in launcher');assert.equal(await windows().count(),0);
      assert.equal((await page.request.get(`${origin}/app-assets/${PACKAGE}/1.0.0/ui/index.html`)).status(),404);
    }
    console.log(`PASS browser ${external?'native VM':'mock installer'}: discovery, icon, SDK, isolation/CSP, theme, notification/dialog, unsupported capabilities, multiple windows, drag/resize/min/max, pin/search/keyboard/mobile${external?'':', enable/disable/uninstall'}`);
  }catch(error){console.error(output);throw error;}
  finally{
    if(browser)await browser.close();
    if(server){server.kill('SIGTERM');await Promise.race([new Promise(resolve=>server.once('exit',resolve)),delay(5000)]);if(server.exitCode===null)server.kill('SIGKILL');}
    if(root)fs.rmSync(root,{recursive:true,force:true});
  }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
