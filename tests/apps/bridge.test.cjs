'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {webcrypto}=require('node:crypto');
const ROOT=path.resolve(__dirname,'../..');
function load(name){const context={module:{exports:{}},AbortController,Uint8Array,console,Date};vm.runInNewContext(fs.readFileSync(path.join(ROOT,name),'utf8'),context);return context.module.exports;}
const {Bridge}=load('web/desktop/apps/bridge.js');
const {Registry}=load('web/desktop/apps/registry.js');
const plain=value=>JSON.parse(JSON.stringify(value));
const tick=()=>new Promise(resolve=>setTimeout(resolve,0));
function metadata(){return {id:'zpk:org.example.app/main',package_id:'org.example.app',desktop_id:'main',version:'1.0.0',title:'Example',categories:['utilities'],window:{width:760,height:600,min_width:320,min_height:240,resizable:true},multiple_instances:true,notifications:true,revision:'a'.repeat(64),entry_url:'/app-assets/org.example.app/1.0.0/ui/index.html',icon_url:'/app-assets/org.example.app/1.0.0/assets/icon.png'};}
function snapshot(apps=[metadata()]){return {schema_version:1,status:'ready',revision:'a'.repeat(64),authentication:{mode:'local-development',authenticated:false,required_for_remote:true},apps};}
function setup(overrides={}){
  const calls=[],messages=[];
  const app={id:'zpk:org.example.app/main',window:{resizable:true},notifications:true,...overrides.app};
  const rootWindow={addEventListener(){},removeEventListener(){}};
  const frame={isConnected:true,contentWindow:{postMessage(data,target){messages.push({data,target});}},addEventListener(type,listener){if(type==='load')this.load=listener;},removeEventListener(){}};
  const bridge=new Bridge({window:rootWindow,crypto:webcrypto,isCurrent:()=>true,getTheme:()=>({mode:'dark',accent:'sea',motion:'normal'}),setTitle:(...a)=>calls.push(['title',...a]),setSize:(key,size)=>{calls.push(['size',key,size]);return size;},notify:(...a)=>calls.push(['notice',...a]),confirm:()=>true,...overrides});
  const context=bridge.attach(frame,app,'window-1');frame.load();
  const event=(data,more={})=>({source:frame.contentWindow,origin:'null',ports:[],data,...more});
  const send=(data,more)=>bridge.receive(event(data,more));
  const session=()=>({instanceId:context.instanceId,nonce:context.nonce,appId:app.id});
  const envelope=(type,rest={})=>({protocol:'zion-app',version:1,type,...session(),...rest});
  const ready=async(nonce='a'.repeat(32))=>{await send({protocol:'zion-app',version:1,type:'hello',clientNonce:nonce});await send(envelope('ack',{clientNonce:nonce}));assert.equal(context.state,'ready');};
  const request=async(method,params={},extra={})=>{await send(envelope('request',{requestId:`r${context.last+1}`,method,params,...extra}));return messages.filter(m=>m.data.type==='response').at(-1)?.data;};
  return {bridge,frame,context,app,calls,messages,send,event,envelope,session,ready,request};
}
test('dynamic registry merges builtins and installed entries and removes stale apps',()=>{
  const registry=new Registry([{id:'storage',title:'Storage'}]);const first=registry.replace(snapshot());assert.equal(first.added.length,1);assert.equal(registry.all().length,2);assert.equal(registry.get(metadata().id).native,true);
  assert.equal(registry.replace(snapshot()).changed.length,0);const changed=metadata();changed.revision='b'.repeat(64);assert.equal(registry.replace(snapshot([changed])).changed.length,1);assert.equal(registry.replace(snapshot([])).removed.length,1);assert.equal(registry.all().length,1);
});
test('registry rejects malformed metadata, unsafe URLs, collisions and unsupported versions',()=>{
  for(const mutate of [x=>x.schema_version=2,x=>x.apps[0].entry_url='/api/storage',x=>x.apps[0].entry_url='/app-assets/org.example.app/1.0.0/ui/../secret.html',x=>x.apps[0].icon_url='https://evil/icon.png',x=>x.apps[0].title='<script>\u0001',x=>x.apps[0].id='storage',x=>x.apps.push(x.apps[0]),x=>x.apps[0].window.width=-1,x=>x.authentication.authenticated=true]){const registry=new Registry([{id:'storage'}]);const value=snapshot();mutate(value);assert.throws(()=>registry.replace(value));assert.equal(registry.all().length,1);}
});
test('handshake binds source, opaque origin, nonce and app instance before effects',async()=>{
  const h=setup();await h.send(h.envelope('request',{requestId:'r1',method:'window.setTitle',params:{title:'early'}}));assert.equal(h.calls.length,0);await h.ready();
  for(const more of [{source:{}},{origin:'http://localhost:8080'},{ports:[{}]}])await h.send(h.envelope('request',{requestId:'r1',method:'window.setTitle',params:{title:'forged'}}),more);
  for(const extra of [{nonce:'0'.repeat(64)},{instanceId:'wrong'},{appId:'zpk:org.example.other/main'},{version:2},{protocol:'wrong'},{root:true}])await h.request('window.setTitle',{title:'forged'},extra);
  assert.equal(h.calls.length,0);const response=await h.request('window.setTitle',{title:'Owned window'});assert.equal(response.ok,true);assert.deepEqual(plain(h.calls),[['title','window-1','Owned window']]);assert.ok(h.messages.every(m=>m.target==='*'));
});
test('unsupported hello version produces a typed compatibility rejection',async()=>{
  const h=setup();await h.send({protocol:'zion-app',version:22,type:'hello',clientNonce:'c'.repeat(32)});assert.equal(h.messages.at(-1).data.error.code,'UNSUPPORTED_VERSION');assert.equal(h.context.state,'hello');
});
test('requests are schema checked, monotonic and do not dispatch arbitrary capabilities',async()=>{
  const h=setup();await h.ready();await h.request('window.setTitle',{title:'one'},{requestId:'r1'});await h.request('window.setTitle',{title:'replay'},{requestId:'r1'});assert.equal(h.calls.length,1);assert.equal(h.messages.at(-1).data.error.code,'REPLAY');
  for(const [method,params,code]of [['window.setTitle',{title:'',extra:true},'INVALID_ARGUMENT'],['window.setSize',{width:-1,height:400},'INVALID_ARGUMENT'],['system.shell',{command:'id'},'UNSUPPORTED_METHOD'],['packages.install',{path:'/dev/vdb'},'UNSUPPORTED_METHOD'],['__proto__',{},'UNSUPPORTED_METHOD']])assert.equal((await h.request(method,params)).error.code,code);
  assert.equal(h.calls.length,1);
});
test('permission declarations cannot be supplied by an RPC caller',async()=>{
  const h=setup({app:{notifications:false,window:{resizable:false}}});await h.ready();assert.equal((await h.request('notifications.show',{title:'hello',message:'world',permission:true})).error.code,'DENIED');assert.equal((await h.request('window.setSize',{width:800,height:600})).error.code,'DENIED');const caps=(await h.request('system.getCapabilities')).result;assert.equal(caps.methods['notifications.show'],false);assert.equal(caps.methods['window.setSize'],false);assert.equal(h.calls.length,0);
});
test('unavailable settings and directories remain typed failures without persistence',async()=>{
  const h=setup();await h.ready();for(const method of ['settings.get','settings.set','files.pickDirectory'])assert.equal((await h.request(method,method==='files.pickDirectory'?{}:{key:'test',value:'x'})).error.code,'UNAVAILABLE');const caps=(await h.request('system.getCapabilities')).result;assert.equal(caps.settingsPersistence,false);assert.equal(caps.nativeBroker,false);assert.equal(h.calls.length,0);
});
test('theme events require a subscription and stop after unsubscribe or detach',async()=>{
  const h=setup();await h.ready();h.bridge.broadcastTheme({mode:'light',accent:'sea',motion:'normal'});assert.equal(h.messages.filter(m=>m.data.type==='event').length,0);await h.request('theme.subscribe');h.bridge.broadcastTheme({mode:'dark',accent:'blue',motion:'reduced'});assert.equal(h.messages.filter(m=>m.data.type==='event').length,1);await h.request('theme.unsubscribe');h.bridge.broadcastTheme({mode:'light',accent:'violet',motion:'normal'});assert.equal(h.messages.filter(m=>m.data.type==='event').length,1);h.bridge.detach(h.frame);assert.equal(h.bridge.contexts.size,0);
});
test('session renewal cannot deliver an old asynchronous response into a new one',async()=>{
  let finish;const h=setup({confirm:()=>new Promise(resolve=>{finish=resolve;})});await h.ready();const old=h.context.nonce;const pending=h.request('dialog.confirm',{title:'Decision',message:'Continue?'});await tick();await h.send(h.envelope('disconnect'));assert.notEqual(h.context.nonce,old);await h.ready('b'.repeat(32));await h.request('system.getCapabilities');const before=h.messages.filter(m=>m.data.type==='response').length;finish(true);await pending;assert.equal(h.messages.filter(m=>m.data.type==='response').length,before);assert.equal(h.messages.filter(m=>m.data.type==='response').at(-1).data.result.protocol,1);
});
test('a second iframe load revokes the context rather than trusting the surviving WindowProxy',async()=>{
  const blocked=[];const h=setup({navigationBlocked:key=>blocked.push(key)});await h.ready();const old=h.envelope('request',{requestId:'r1',method:'window.setTitle',params:{title:'navigated'}});h.frame.load();assert.equal(h.bridge.contexts.size,0);assert.deepEqual(blocked,['window-1']);await h.send(old);assert.equal(h.calls.length,0);
});
test('closed, removed, disconnected and over-budget frames cannot continue dispatch',async()=>{
  const h=setup();await h.ready();for(let n=0;n<60;n++)await h.request('window.setTitle',{title:`${n}`});assert.ok(h.calls.length>0&&h.calls.length<=38);h.bridge.detach(h.frame);const before=h.calls.length;await h.send(h.envelope('request',{requestId:'r100',method:'window.setTitle',params:{title:'closed'}}));assert.equal(h.calls.length,before);
  const gone=setup({isCurrent:()=>false});await gone.send({protocol:'zion-app',version:1,type:'hello',clientNonce:'a'.repeat(32)});assert.equal(gone.messages.length,0);
});
function sdkPair(){
  const h=setup();const listeners=new Map();const parent={postMessage(data,target){assert.equal(target,'http://localhost:8080');const cloned=structuredClone(data);queueMicrotask(()=>{void h.send(cloned);});}};
  const child={parent,location:{href:'http://localhost:8080/app-assets/org.example.app/1.0.0/ui/index.html'},addEventListener(type,listener){listeners.set(type,listener);},removeEventListener(type,listener){if(listeners.get(type)===listener)listeners.delete(type);}};
  h.frame.contentWindow.postMessage=(data,target)=>{assert.equal(target,'*');queueMicrotask(()=>listeners.get('message')?.({source:parent,origin:'http://localhost:8080',ports:[],data:structuredClone(data)}));};
  const context=vm.createContext({window:child,crypto:webcrypto,URL,Uint8Array,setTimeout,clearTimeout,setInterval,clearInterval});vm.runInContext(fs.readFileSync(path.join(ROOT,'web/zion-app-sdk/dist/zion-app-sdk.js'),'utf8'),context);
  return {h,sdk:context.ZionApp,child,listeners,parent};
}
test('real SDK code connects, performs typed APIs, subscribes, disconnects and reconnects',async()=>{
  const {h,sdk}=sdkPair();const app=await sdk.connect();assert.equal(app.id,h.app.id);await app.window.setTitle('SDK title');assert.equal(h.calls[0][2],'SDK title');assert.equal((await app.theme.get()).mode,'dark');
  const themes=[];const unsubscribe=await app.theme.subscribe(value=>themes.push(value.mode));h.bridge.broadcastTheme({mode:'light',accent:'sea',motion:'normal'});await tick();assert.deepEqual(themes,['dark','light']);unsubscribe();await tick();
  for(const action of [()=>app.files.pickDirectory(),()=>app.settings.get('welcome')])await assert.rejects(action,error=>error instanceof sdk.ZionError&&error.code==='UNAVAILABLE');
  await assert.rejects(()=>app.settings.set('invalid',()=>{}),error=>error.code==='INVALID_ARGUMENT');
  app.disconnect();await tick();const second=await sdk.connect();assert.notEqual(second.instanceId,app.instanceId);second.disconnect();h.bridge.dispose();
});
test('SDK ignores wrong parent/source responses and rejects use outside an embedded app',async()=>{
  const {h,sdk,listeners}=sdkPair();const app=await sdk.connect();const before=h.calls.length;
  listeners.get('message')({source:{},origin:'http://localhost:8080',ports:[],data:{protocol:'zion-app',version:1,type:'event',event:'theme.changed',value:{mode:'bad'}}});assert.equal(h.calls.length,before);app.disconnect();h.bridge.dispose();
  const own={location:{href:'http://localhost:8080/'}};own.parent=own;const context=vm.createContext({window:own,crypto:webcrypto,URL,Uint8Array,setTimeout,clearTimeout,setInterval,clearInterval});vm.runInContext(fs.readFileSync(path.join(ROOT,'web/zion-app-sdk/dist/zion-app-sdk.js'),'utf8'),context);await assert.rejects(()=>context.ZionApp.connect(),e=>e.code==='UNAVAILABLE');
});
test('ES module distribution exports the same typed browser-only API',async()=>{
  const sdk=await import(require('node:url').pathToFileURL(path.join(ROOT,'web/zion-app-sdk/dist/zion-app-sdk.mjs')).href);
  assert.equal(sdk.protocolVersion,1);assert.equal(typeof sdk.connect,'function');await assert.rejects(()=>sdk.connect(),error=>error instanceof sdk.ZionError&&error.code==='UNAVAILABLE');
});
