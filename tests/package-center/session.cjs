'use strict';
// System-session UI fixtures only. Real enrollment/cookies/broker are tested in QEMU.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {chromium}=require(path.resolve(__dirname,'../../web/zion-app-sdk/node_modules/playwright'));
const ROOT=path.resolve(__dirname,'../..');
async function main(){const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1100,height:900}});let configured=false,loggedIn=false,role='administrator',enrollments=0,logins=0,packageReads=0;
 await page.route('http://session.test/**',async route=>{const r=route.request(),url=new URL(r.url());let data;
  if(url.pathname==='/')return route.fulfill({contentType:'text/html',body:'<!doctype html><link rel="stylesheet" href="/desktop-session.css"><div id="app"><button id="zion-account"></button><div id="mount"></div></div><script src="/desktop-session.js"></script><script src="/package-center.js"></script><script src="/start.js"></script>'});
  if(url.pathname==='/desktop-session.css')return route.fulfill({contentType:'text/css',body:fs.readFileSync(path.join(ROOT,'web/desktop/session.css'),'utf8')});
  if(url.pathname==='/desktop-session.js'||url.pathname==='/package-center.js')return route.fulfill({contentType:'application/javascript',body:fs.readFileSync(path.join(ROOT,url.pathname==='/package-center.js'?'web/desktop/apps/package-center.js':'web/desktop/session.js'),'utf8')});
  if(url.pathname==='/start.js')return route.fulfill({contentType:'application/javascript',body:"ZionSession.initialize();ZionPackageCenter.mount(document.querySelector('#mount'),{toast:()=>{},openApp:()=>{},refresh:async()=>{}});"});
  if(url.pathname==='/api/auth/status')data={available:true,configured};
  else if(url.pathname==='/api/auth/enroll'){assert.equal(configured,false);assert.deepEqual(Object.keys(r.postDataJSON()).sort(),['password','username']);assert.equal(r.postDataJSON().username,'owner');configured=true;enrollments++;data={configured:true};}
  else if(url.pathname==='/api/auth/login'){assert.equal(r.postDataJSON().username,'owner');assert.ok(r.postDataJSON().password.length>=12);loggedIn=true;logins++;data={role,username:'owner',csrf:'fixture-csrf'};}
  else if(url.pathname==='/api/auth/logout'){assert.equal(r.headers()['x-zion-csrf'],'fixture-csrf');loggedIn=false;data={logged_out:true};}
  else if(url.pathname==='/api/auth/session'){if(!loggedIn)return route.fulfill({status:401,json:{ok:false,message:'Sign in to ZionOS'}});data={role,username:'owner',csrf:'fixture-csrf'};}
  else if(url.pathname.startsWith('/api/packages')){if(!loggedIn)return route.fulfill({status:401,json:{ok:false,message:'Session expired'}});packageReads++;data=url.pathname.endsWith('/catalog')?{packages:[]}:{apps:[]};}
  else throw new Error('Unexpected route '+url.pathname);
  return route.fulfill({json:{ok:true,data}});
 });
 await page.goto('http://session.test');await page.locator('#zion-password-confirm').waitFor();assert.equal(await page.locator('#app').evaluate(n=>n.inert),true);assert.equal(await page.locator('#pc-password,#pc-setup-code,#pc-account').count(),0);await page.locator('#zion-username').fill('owner');await page.locator('#zion-password').fill('fixture-password-only');await page.locator('#zion-password-confirm').fill('different-password');await page.locator('#zion-sign-in').click();await page.getByText('Passwords do not match',{exact:true}).waitFor();assert.equal(enrollments,0);await page.locator('#zion-password').fill('fixture-password-only');await page.locator('#zion-password-confirm').fill('fixture-password-only');await page.locator('#zion-sign-in').click();await page.locator('.pc-toolbar').waitFor();assert.equal(enrollments,1);assert.equal(logins,1);assert.equal(await page.locator('#zion-auth').count(),0);assert.equal(await page.locator('.package-center input[type=password]').count(),0);assert.ok(packageReads>0);
 loggedIn=false;await page.locator('.package-center').getByRole('button',{name:'Refresh',exact:true}).click();await page.locator('#zion-password').waitFor();assert.equal(await page.locator('#zion-password-confirm').count(),0);await page.locator('#zion-username').fill('owner');await page.locator('#zion-password').fill('fixture-password-only');await page.locator('#zion-sign-in').click();await page.locator('.pc-toolbar').waitFor();assert.equal(logins,2);await page.locator('#zion-account').click();await page.locator('#zion-password').waitFor();assert.equal(loggedIn,false);
 // UI role fixture proves Package Center cannot manufacture an admin session.
 loggedIn=true;role='user';const before=packageReads;await page.reload();await page.getByText('Your ZionOS account does not have package administration rights',{exact:false}).waitFor();assert.equal(packageReads,before);assert.equal(await page.locator('.package-center input[type=password]').count(),0);
 console.log('PASS shared ZionOS session UI: code-free owner setup, single login, no Package Center credentials, expiry, global logout and non-admin refusal');
 }finally{await browser.close();}}
main().catch(e=>{console.error(e);process.exitCode=1;});
