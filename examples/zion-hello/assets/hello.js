'use strict';
import {connect} from './zion-app-sdk.mjs';
(async()=>{
  const byId=id=>document.getElementById(id);
  const report=async action=>{try{const result=await action();byId('result').textContent=typeof result==='string'?result:JSON.stringify(result,null,2);}catch(e){byId('result').textContent=`${e.code||'UNAVAILABLE'}: ${e.message}`;}};
  try{
    const app=await connect();
    await app.window.setTitle('Zion Hello · Your space');
    const apply=theme=>{document.documentElement.dataset.theme=theme.mode;document.documentElement.dataset.accent=theme.accent;byId('theme').textContent=`Theme: ${theme.mode} · ${theme.accent}`;};
    apply(await app.theme.get());await app.theme.subscribe(apply);
    byId('connection').textContent='Connected to ZionOS';
    document.querySelectorAll('button').forEach(button=>{button.disabled=false;});
    byId('notify').onclick=()=>report(()=>app.notifications.show({title:'Hello from Zion',message:'Your native application is part of your space.'}));
    byId('resize').onclick=()=>report(()=>app.window.setSize({width:820,height:640}));
    byId('confirm').onclick=()=>report(async()=>`Confirmed: ${await app.dialog.confirm({title:'A small decision',message:'Keep creating in your space?'})}`);
    byId('directory').onclick=()=>report(()=>app.files.pickDirectory());
    byId('settings').onclick=()=>report(()=>app.settings.get('welcome'));
    byId('capabilities').onclick=()=>report(()=>app.system.getCapabilities());
  }catch(e){byId('connection').textContent='Desktop connection unavailable';byId('result').textContent=`${e.code||'UNAVAILABLE'}: ${e.message}`;}
})();
