import { connect, type App, type Size, type Theme, type Capabilities } from '../../web/zion-app-sdk/dist/index';
async function typedApplication(): Promise<void> {
  const app: App = await connect();
  await app.window.setTitle('Typed application');
  const size: Size = await app.window.setSize({width:760,height:600});
  const theme: Theme = await app.theme.get();
  const unsubscribe: () => void = await app.theme.subscribe(value => { console.log(value.mode); });
  const confirmed: boolean = await app.dialog.confirm({title:'Confirm',message:'Continue?'});
  const capabilities: Capabilities = await app.system.getCapabilities();
  await app.notifications.show({title:'Ready',message:`${size.width} ${theme.mode} ${confirmed} ${capabilities.protocol}`});
  unsubscribe();app.disconnect();
}
void typedApplication;
