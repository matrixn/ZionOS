'use strict';
// Actual production Core/broker/OpenRC in QEMU. No package/API mocks.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {chromium,request}=require(path.resolve(__dirname,'../../web/zion-app-sdk/node_modules/playwright'));
const folder=path.resolve(process.argv[2]),phase=process.argv[3],origin='http://127.0.0.1:8080';
const ID='org.zionos.zion-hello',APP=`zpk:${ID}/main`;
const secret=JSON.parse(fs.readFileSync(path.join(folder,'credentials.json'),'utf8'));
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function wait(fn,message,timeout=50000){const end=Date.now()+timeout;let error;while(Date.now()<end){try{if(await fn())return;}catch(e){error=e;}await delay(120);}throw new Error(`${message}${error?`: ${error.message}`:''}`);}
async function main(){let browser,page;try{
 browser=await chromium.launch({headless:true});const context=await browser.newContext({viewport:{width:1440,height:1000}});page=await context.newPage();
 await page.goto(origin);await page.locator('[data-dock="packages"]').click();const pc=page.locator('.package-center');await wait(()=>pc.locator('#pc-password').count(),'Administrator UI missing');
 const unauthorized=await request.newContext();try{
  assert.equal((await unauthorized.get(`${origin}/api/packages`)).status(),401);
  assert.equal((await unauthorized.post(`${origin}/api/packages/action`,{data:{id:ID,action:'start'},headers:{Origin:origin}})).status(),401);
  assert.equal((await unauthorized.post(`${origin}/api/auth/login`,{data:{password:'invalid-password-only'},headers:{Origin:'https://attacker.invalid'}})).status(),403);
  assert.equal((await unauthorized.get(`${origin}/api/packages`,{headers:{Cookie:`zion_session=${'a'.repeat(64)}`}})).status(),401);
 }finally{await unauthorized.dispose();}
 if(phase==='first')await pc.locator('#pc-setup-code').fill(secret.code);else assert.equal(await pc.locator('#pc-setup-code').count(),0);
 await pc.locator('#pc-password').fill(secret.password);await pc.locator('#pc-sign-in').click();await wait(()=>pc.locator('[data-pc-tab=installed]').count(),'Administrator login/setup failed');
 const cookie=(await context.cookies()).find(c=>c.name==='zion_session');assert.ok(Boolean(cookie));assert.equal(cookie.httpOnly,true);assert.equal(cookie.sameSite,'Strict');assert.equal(cookie.path,'/api');
 const session=await (await page.request.get(`${origin}/api/auth/session`)).json();assert.equal(session.data.role,'administrator');const csrf=session.data.csrf;
 for(const headers of [{Origin:origin},{Origin:origin,'X-Zion-CSRF':'a'.repeat(64)},{Origin:'http://localhost:8080','X-Zion-CSRF':csrf}])assert.equal((await page.request.post(`${origin}/api/packages/action`,{data:{id:ID,action:'stop'},headers})).status(),403);
 assert.equal((await page.request.post(`${origin}/api/packages/action`,{data:{id:ID,action:'start',authorized:true},headers:{Origin:origin,'X-Zion-CSRF':csrf}})).status(),422);
 const cards=()=>pc.locator(`[data-package-id="${ID}"]`);
 async function state(){const r=await page.request.get(`${origin}/api/packages`);if(r.status()!==200)return null;return (await r.json()).data.apps.find(a=>a.id===ID);}
 async function details(){await wait(()=>cards().count(),'Installed package card missing');await cards().click();await wait(()=>pc.locator('[data-pc-action=uninstall]').count(),'Package detail missing');}
 async function upload(version,failed=false){await pc.locator('[data-pc-tab=upload]').click();await pc.locator('#pc-upload-file').setInputFiles(path.join(folder,`media/hello-${version}.zpk`));await wait(()=>pc.locator('#pc-install-confirm').count(),'Validated preview missing');
  assert.equal(await pc.locator('#pc-install-confirm').isDisabled(),true);await pc.locator('#pc-trust').check();await pc.locator('#pc-approve').check();assert.equal(await pc.locator('#pc-install-confirm').isEnabled(),true);await pc.locator('#pc-install-confirm').click();
  if(failed){await wait(async()=>await pc.locator('.pc-error').count()>0,'Failed upgrade was reported as success');assert.equal((await state()).active,'1.1.0');return;}
  await wait(async()=>{const r=await state();return r?.active===version&&r.state.state==='running';},'Native package did not reach expected version/state');await wait(()=>cards().count(),'UI did not publish installed card');}
 async function lifecycle(action,expected){await details();await pc.locator(`[data-pc-action=${action}]`).click();if(['uninstall','rollback'].includes(action))await pc.locator('.pc-dialog').getByRole('button',{name:'Confirm',exact:true}).click();await wait(async()=>{const r=await state();return expected==='removed'?!r:r?.state.state===expected;},`${action} did not reach ${expected}`);await wait(()=>pc.locator('.pc-detail').count().then(n=>n===0),'Operation did not finish in UI');}
 async function native(version){await wait(()=>page.locator(`.launcher-app[data-open="${APP}"]`).count(),'Launcher entry missing');await page.locator('#brand-launcher').click();await page.locator(`.launcher-app[data-open="${APP}"]`).click();await wait(async()=>{const f=page.frames().find(f=>f.url().includes(`/app-assets/${ID}/${version}/`));return f&&await f.locator('#connection').textContent()==='Connected to ZionOS';},'Native SDK window failed');const win=page.locator(`.os-window[data-app="${APP}"]`);assert.equal(await win.locator('.window-title').textContent(),'Zion Hello · Your space');const f=page.frames().find(f=>f.url().includes(`/app-assets/${ID}/${version}/`));assert.equal(await f.evaluate(()=>{try{return Boolean(parent.document);}catch{return false;}}),false);assert.equal(await f.evaluate(async()=>{try{await fetch('/api/auth/session');return true;}catch{return false;}}),false);await f.locator('#notify').click();await wait(()=>page.locator('#toast-text').textContent().then(s=>s.includes('Your native application')),'Native SDK notification missing');await win.locator('[data-window-action=min]').click();}
 if(phase==='first'){
  await pc.locator('[data-pc-tab=discover]').click();await wait(()=>pc.locator('.pc-card').count(),'Bundled curated catalog missing');assert.ok((await pc.textContent()).includes('Zion Hello'));
  await page.screenshot({path:path.join(folder,'package-center-grid.png'),fullPage:true});await pc.locator('.pc-card').first().click();await wait(()=>pc.locator('[data-pc-action=install]').count(),'Catalog detail missing');assert.ok((await pc.locator('.pc-detail').textContent()).includes('Requirements and compatibility'));assert.equal(await pc.locator('[data-pc-action=uninstall]').count(),0);assert.equal(await pc.locator('#pc-install-confirm').count(),0);await pc.locator('[data-pc-action=install]').click();await wait(()=>pc.locator('#pc-install-confirm').count(),'Curated package preview missing');await page.screenshot({path:path.join(folder,'package-center-permissions.png'),fullPage:true});await pc.getByRole('button',{name:'Cancel',exact:true}).click();
  for(const attack of ['traversal','architecture','hash']){
   const r=await page.request.post(`${origin}/api/packages/preview`,{data:fs.readFileSync(path.join(folder,`media/bad-${attack}.zpk`)),headers:{Origin:origin,'X-Zion-CSRF':csrf,'Content-Type':'application/octet-stream'}});assert.equal(r.status(),422,attack);
  }
  // Actual concurrent native installs, not merely a mock mutex test. The package has
  // no desktop entry; it must never create a fake launcher icon.
  const background='org.zionos.zion-hello-rust';
  const previews=await Promise.all([0,1].map(async()=>{const r=await page.request.post(`${origin}/api/packages/preview`,{data:fs.readFileSync(path.join(folder,'media/background.zpk')),headers:{Origin:origin,'X-Zion-CSRF':csrf,'Content-Type':'application/octet-stream'}});assert.equal(r.status(),200);return(await r.json()).data;}));
  const concurrent=await Promise.all(previews.map(p=>page.request.post(`${origin}/api/packages/install`,{data:{receipt:p.receipt,upgrade:false,permissions:p.manifest.permissions,trusted_local:true,start:true,autostart:true,unix_health:true},headers:{Origin:origin,'X-Zion-CSRF':csrf}})));
  assert.equal(concurrent.filter(r=>r.status()===200).length,1);assert.equal(concurrent.filter(r=>r.status()===409).length,1);
  for(const r of concurrent.filter(r=>r.status()===200)){const id=(await r.json()).data.operation_id;await wait(async()=>{const p=await(await page.request.get(`${origin}/api/packages/operations/${id}`)).json();return p.data?.state==='COMMITTED';},'Concurrent install did not commit');}
  assert.equal((await(await page.request.get(`${origin}/api/apps`)).json()).apps.some(a=>a.package_id===background),false);
  const removed=await page.request.post(`${origin}/api/packages/action`,{data:{id:background,action:'uninstall'},headers:{Origin:origin,'X-Zion-CSRF':csrf}});assert.equal(removed.status(),200);const removal=(await removed.json()).data.operation_id;await wait(async()=> (await(await page.request.get(`${origin}/api/packages/operations/${removal}`)).json()).data?.state==='COMMITTED','Background uninstall failed');
  await upload('1.0.0');await pc.locator('[data-pc-tab=discover]').click();await cards().click();assert.equal(await pc.locator('[data-pc-action=install]').count(),0);assert.equal(await pc.locator('[data-pc-action=uninstall]').count(),1);assert.equal(await pc.locator('[data-pc-action=stop]').count(),1);await pc.locator('[data-pc-tab=installed]').click();await native('1.0.0');await lifecycle('stop','stopped');await lifecycle('start','running');await lifecycle('restart','running');await details();
  await page.screenshot({path:path.join(folder,'package-center-dark.png'),fullPage:true});await page.locator('#theme-toggle').click();await page.screenshot({path:path.join(folder,'package-center-light.png'),fullPage:true});
  await page.setViewportSize({width:390,height:820});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);await page.screenshot({path:path.join(folder,'package-center-mobile.png'),fullPage:true});
 }else if(phase==='second'){
  assert.equal((await state()).active,'1.0.0');assert.equal((await state()).state.state,'running');await native('1.0.0');await upload('1.1.0');await wait(()=>page.locator(`.os-window[data-app="${APP}"]`).count().then(n=>n===0),'Old native window survived upgrade');assert.equal((await page.request.get(`${origin}/app-assets/${ID}/1.0.0/ui/index.html`)).status(),404);await native('1.1.0');await upload('1.2.0',true);
 }else if(phase==='resume'){
  assert.equal((await state()).active,'1.0.0');assert.equal((await state()).state.state,'running');await native('1.0.0');
 }else if(phase==='capacity'){
  const r=await page.request.post(`${origin}/api/packages/preview`,{data:fs.readFileSync(path.join(folder,'media/hello-1.1.0.zpk')),headers:{Origin:origin,'X-Zion-CSRF':csrf,'Content-Type':'application/octet-stream'}});assert.equal(r.status(),507);assert.equal((await r.json()).code,'INSUFFICIENT_STORAGE');
 }else if(phase==='third'){
  assert.equal((await state()).active,'1.1.0');assert.equal((await state()).state.state,'running');await native('1.1.0');await lifecycle('rollback','running');await wait(async()=> (await state())?.active==='1.0.0','Manual executable rollback failed');await native('1.0.0');await lifecycle('uninstall','removed');await wait(()=>page.locator(`.launcher-app[data-open="${APP}"]`).count().then(n=>n===0),'Uninstalled app remains in launcher');assert.equal((await page.request.get(`${origin}/app-assets/${ID}/1.0.0/ui/index.html`)).status(),404);
  await pc.locator('[data-pc-tab=discover]').click();await cards().click();assert.equal(await pc.locator('[data-pc-action=install]').count(),1);assert.equal(await pc.locator('[data-pc-action=uninstall]').count(),0);assert.equal(await pc.locator('[data-pc-action=stop]').count(),0);await pc.locator('#pc-account').click();await wait(()=>pc.locator('#pc-password').count(),'Logout UI did not appear');assert.equal((await page.request.get(`${origin}/api/packages`)).status(),401);const replay=await request.newContext();assert.equal((await replay.get(`${origin}/api/packages`,{headers:{Cookie:`zion_session=${cookie.value}`}})).status(),401);await replay.dispose();
 }else throw new Error('unknown browser phase');
 console.log(`PASS real Package Center browser ${phase}: authentication/CSRF and graphical native lifecycle`);
 }catch(e){if(page){await page.screenshot({path:path.join(folder,'failure-'+phase+'.png'),fullPage:true}).catch(()=>{});fs.writeFileSync(path.join(folder,'failure-'+phase+'.txt'),await page.locator('.package-center').textContent().catch(()=>'UI unavailable'));}throw e;}finally{if(browser)await browser.close();}}
main().catch(e=>{console.error(e.message);process.exitCode=1;});
