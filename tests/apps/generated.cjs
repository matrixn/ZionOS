'use strict';
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict'),{execFileSync}=require('node:child_process');
const root=path.resolve(__dirname,'../..'),sdk=path.join(root,'web/zion-app-sdk');
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'zion-sdk-generated-'));
try{
  execFileSync(process.execPath,[path.join(sdk,'node_modules/typescript/bin/tsc'),'-p',path.join(sdk,'tsconfig.json'),'--outDir',temporary],{stdio:'inherit'});
  execFileSync(process.execPath,[path.join(sdk,'build-modules.cjs'),temporary],{stdio:'inherit'});
  for(const name of ['zion-app-sdk.js','zion-app-sdk.mjs','zion-app-sdk.d.ts','index.d.ts'])assert.deepEqual(fs.readFileSync(path.join(sdk,'dist',name)),fs.readFileSync(path.join(temporary,name)),`SDK distribution drift: ${name}; run npm --prefix web/zion-app-sdk run build`);
  assert.deepEqual(fs.readFileSync(path.join(sdk,'dist/zion-app-sdk.mjs')),fs.readFileSync(path.join(root,'examples/zion-hello/assets/zion-app-sdk.mjs')),'Example SDK copy drift; run python3 examples/zion-hello/build-assets.py');
  execFileSync(process.execPath,[path.join(sdk,'node_modules/typescript/bin/tsc'),'--noEmit','--strict','--skipLibCheck','--target','ES2020','--module','ES2020','--moduleResolution','bundler',path.join(root,'tests/apps/sdk-types.ts')],{stdio:'inherit'});
  console.log('PASS SDK reproducible JS/declarations, example distribution and TypeScript consumer');
}finally{fs.rmSync(temporary,{recursive:true,force:true});}
