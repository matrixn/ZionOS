/* Zion App bridge protocol v1. Own-window UI capabilities only; no system or file broker. */
(function(root,factory) {
  const api=factory();
  if (typeof module==='object' && module.exports) module.exports=api;
  else root.ZionAppBridge=api;
})(typeof window==='undefined'?globalThis:window,function(){
  'use strict';
  const object=v=>v!==null&&typeof v==='object'&&!Array.isArray(v);
  const keys=(v,n)=>object(v)&&Object.keys(v).sort().join(',')===n.slice().sort().join(',');
  const text=(v,n)=>typeof v==='string'&&v.trim().length>0&&v.length<=n&&!/[\u0000-\u001f\u007f-\u009f]/.test(v);
  const isTheme=v=>keys(v,['mode','accent','motion'])&&['dark','light'].includes(v.mode)&&['sea','blue','violet'].includes(v.accent)&&['normal','reduced'].includes(v.motion);
  class Failure extends Error { constructor(code,message){super(message);this.code=code;} }
  class Bridge {
    constructor(options){this.options=options;this.contexts=new Map();this.lastTheme='';this.listener=e=>{void this.receive(e);};options.window.addEventListener('message',this.listener);}
    random(n){return Array.from(this.options.crypto.getRandomValues(new Uint8Array(n)),b=>b.toString(16).padStart(2,'0')).join('');}
    uuid(){const b=this.options.crypto.getRandomValues(new Uint8Array(16));b[6]=(b[6]&15)|64;b[8]=(b[8]&63)|128;const h=Array.from(b,x=>x.toString(16).padStart(2,'0')).join('');return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;}
    attach(frame,app,key){
      const context={frame,app,key,state:'loading',loaded:false,instanceId:'',nonce:'',clientNonce:'',last:0,subscribed:false,count:0,period:0,lastNotification:-Infinity,abort:null};
      const reset=()=>{if(context.abort)context.abort.abort();Object.assign(context,{state:'hello',instanceId:this.uuid(),nonce:this.random(32),clientNonce:'',last:0,subscribed:false,count:0,period:0,abort:new AbortController()});};
      // A WindowProxy survives document navigation, while an opaque origin remains
      // serialized as "null". Never offer a new session to a navigated document.
      const load=()=>{if(context.loaded){this.detach(frame);this.options.navigationBlocked?.(key);return;}context.loaded=true;reset();};
      context.reset=reset;context.load=load;frame.addEventListener('load',load);this.contexts.set(frame,context);return context;
    }
    detach(frame){const c=this.contexts.get(frame);if(!c)return;c.state='closed';c.nonce='';if(c.abort)c.abort.abort();frame.removeEventListener('load',c.load);this.contexts.delete(frame);}
    dispose(){for(const frame of [...this.contexts.keys()])this.detach(frame);this.options.window.removeEventListener('message',this.listener);}
    active(c){return c.state!=='closed'&&c.frame.isConnected&&this.options.isCurrent(c.app);}
    send(c,value){if(this.active(c))c.frame.contentWindow.postMessage({protocol:'zion-app',version:1,instanceId:c.instanceId,nonce:c.nonce,appId:c.app.id,...value},'*');}
    capabilities(c){return {protocol:1,methods:{'window.setTitle':true,'window.setSize':c.app.window.resizable,'notifications.show':c.app.notifications,'theme.get':true,'theme.subscribe':true,'theme.unsubscribe':true,'dialog.confirm':true,'system.getCapabilities':true,'settings.get':false,'settings.set':false,'files.pickDirectory':false},settingsPersistence:false,nativeBroker:false,isolation:'sandboxed-opaque-iframe'};}
    async receive(event){
      try{
        if(event.origin!=='null'||event.ports?.length||!object(event.data))return;
        const c=[...this.contexts.values()].find(c=>event.source===c.frame.contentWindow);
        if(!c||!this.active(c)||c.state==='loading')return;
        const m=event.data;
        if(JSON.stringify(m).length>16384||m.protocol!=='zion-app')return;
        const now=Date.now();if(now-c.period>=1000){c.period=now;c.count=0;}if(++c.count>40)return;
        if(m.version!==1){if(m.type==='hello'&&keys(m,['protocol','version','type','clientNonce'])&&typeof m.clientNonce==='string'&&/^[a-f0-9]{32}$/.test(m.clientNonce))this.send(c,{type:'rejected',clientNonce:m.clientNonce,error:{code:'UNSUPPORTED_VERSION',message:'Only Zion bridge protocol v1 is supported.'}});return;}
        if(m.type==='hello'){
          if(!keys(m,['protocol','version','type','clientNonce'])||typeof m.clientNonce!=='string'||!/^[a-f0-9]{32}$/.test(m.clientNonce)||c.state==='ready')return;
          if(c.clientNonce&&m.clientNonce!==c.clientNonce)return;
          c.clientNonce=m.clientNonce;c.state='offered';this.send(c,{type:'offer',clientNonce:m.clientNonce});return;
        }
        if(m.instanceId!==c.instanceId||m.nonce!==c.nonce||m.appId!==c.app.id)return;
        if(m.type==='disconnect'&&keys(m,['protocol','version','type','instanceId','nonce','appId'])){c.reset();return;}
        if(m.type==='ack'){
          if(!keys(m,['protocol','version','type','instanceId','nonce','appId','clientNonce'])||c.state!=='offered'||m.clientNonce!==c.clientNonce)return;
          c.state='ready';this.send(c,{type:'connected'});return;
        }
        if(c.state!=='ready'||m.type!=='request'||!keys(m,['protocol','version','type','instanceId','nonce','appId','requestId','method','params'])||
          typeof m.requestId!=='string'||!/^r[1-9][0-9]{0,9}$/.test(m.requestId)||typeof m.method!=='string'||m.method.length>64||!object(m.params))return;
        const sequence=Number(m.requestId.slice(1));
        if(sequence>2147483647)return;
        if(sequence<=c.last){this.send(c,{type:'response',requestId:m.requestId,ok:false,error:{code:'REPLAY',message:'Request ID already consumed.'}});return;}
        c.last=sequence;
        const session=c.nonce;
        try{const result=await this.perform(c,m.method,m.params);if(c.state==='ready'&&c.nonce===session)this.send(c,{type:'response',requestId:m.requestId,ok:true,result});}
        catch(error){if(c.state==='ready'&&c.nonce===session)this.send(c,{type:'response',requestId:m.requestId,ok:false,error:{code:error instanceof Failure?error.code:'UNAVAILABLE',message:error instanceof Failure?error.message:'Desktop capability unavailable.'}});}
      }catch{/* Malformed structured messages never enter a capability handler. */}
    }
    async perform(c,method,p){
      const empty=()=>{if(!keys(p,[]))throw new Failure('INVALID_ARGUMENT','This method takes no parameters.');};
      switch(method){
        case 'window.setTitle':if(!keys(p,['title'])||!text(p.title,128))throw new Failure('INVALID_ARGUMENT','Title must be 1..128 plain-text characters.');this.options.setTitle(c.key,p.title);return null;
        case 'window.setSize':
          if(!c.app.window.resizable)throw new Failure('DENIED','This application declares a fixed window size.');
          if(!keys(p,['width','height'])||!Number.isInteger(p.width)||!Number.isInteger(p.height)||p.width<160||p.width>7680||p.height<120||p.height>4320)throw new Failure('INVALID_ARGUMENT','Invalid window dimensions.');
          return this.options.setSize(c.key,p);
        case 'notifications.show':
          if(!c.app.notifications)throw new Failure('DENIED','notifications permission was not approved for this entry.');
          if(!keys(p,['title','message'])||!text(p.title,100)||!text(p.message,1000))throw new Failure('INVALID_ARGUMENT','Notification text exceeds limits.');
          if(Date.now()-c.lastNotification<2000)throw new Failure('BUSY','Notification rate limit.');c.lastNotification=Date.now();this.options.notify(c.app,p);return {shown:true};
        case 'theme.get':empty();return this.options.getTheme();
        case 'theme.subscribe':empty();c.subscribed=true;return this.options.getTheme();
        case 'theme.unsubscribe':empty();c.subscribed=false;return null;
        case 'dialog.confirm':if(!keys(p,['title','message'])||!text(p.title,100)||!text(p.message,2000))throw new Failure('INVALID_ARGUMENT','Invalid confirmation text.');return this.options.confirm(c.app,p,c.abort.signal);
        case 'system.getCapabilities':empty();return this.capabilities(c);
        case 'settings.get':case 'settings.set':throw new Failure('UNAVAILABLE','Persistent application settings are not implemented.');
        case 'files.pickDirectory':empty();throw new Failure('UNAVAILABLE','Directory access requires a future authenticated permission broker.');
        default:throw new Failure('UNSUPPORTED_METHOD','Unsupported bridge method.');
      }
    }
    broadcastTheme(value){
      if(!isTheme(value))throw new Error('Invalid desktop theme');
      const serialized=JSON.stringify(value);if(serialized===this.lastTheme)return;this.lastTheme=serialized;
      for(const c of this.contexts.values())if(c.state==='ready'&&c.subscribed)this.send(c,{type:'event',event:'theme.changed',value});
    }
  }
  return {Bridge,Failure,isTheme};
});
