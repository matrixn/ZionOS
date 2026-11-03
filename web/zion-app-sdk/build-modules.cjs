'use strict';
// Deterministic distribution wrappers; no bundler or runtime dependency is needed.
const fs=require('node:fs'),path=require('node:path');
const dir=process.argv[2]||path.join(__dirname,'dist');
const js=fs.readFileSync(path.join(dir,'zion-app-sdk.js'),'utf8');
const types=fs.readFileSync(path.join(dir,'zion-app-sdk.d.ts'),'utf8');
fs.writeFileSync(path.join(dir,'zion-app-sdk.mjs'),`${js}\nexport const connect = ZionApp.connect;\nexport const ZionError = ZionApp.ZionError;\nexport const protocolVersion = ZionApp.protocolVersion;\n`);
fs.writeFileSync(path.join(dir,'index.d.ts'),`${types}\nexport = ZionApp;\nexport as namespace ZionApp;\n`);
