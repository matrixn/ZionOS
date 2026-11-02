const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../web/desktop/app.js'), 'utf8');

test('Single/Mirror/RAID5 preview capacities and rejected inputs', () => {
  const code = source.slice(source.indexOf('function capacityPreview('), source.indexOf('function renderPoolWindow('));
  const context = vm.createContext({});
  vm.runInContext(code, context);
  const preview = context.capacityPreview;
  const disks = sizes => sizes.map((size_bytes,index) => ({name:`disk${index}`,size_bytes}));
  assert.equal(preview('single',disks([1000])).estimated,1000);
  assert.equal(preview('mirror',disks([1000,2000])).estimated,1000);
  assert.equal(preview('raid5',disks([1000,2000,3000])).estimated,2000);
  for (const [layout,sizes] of [['single',[]],['single',[1,2]],['mirror',[1]],['raid5',[1,2]],['bad',[1,2,3]],['single',[NaN]],['single',[-1]],['raid5',[Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER]]])
    assert.equal(preview(layout,disks(sizes)).valid,false);
  assert.equal(preview('mirror',[{name:'same',size_bytes:1},{name:'same',size_bytes:2}]).valid,false);
});

function labContext(fetch) {
  const nodes = new Map(['totals','reasons','commands','recovery'].map(key => [key,{
    textContent:'OLD_ELIGIBLE_RESULT', children:['OLD_ELIGIBLE_RESULT'],
    replaceChildren(...items) { this.children=items; this.textContent=items.map(i=>i.textContent||'').join(''); },
    append(...items) { this.children.push(...items); }
  }]));
  const win = {querySelector(selector) { return nodes.get(selector.slice('[data-lab-'.length,-1)); }};
  const context=vm.createContext({fetch, AbortSignal, windows:new Map([['storage',win]]),
    storageElement:(tag,cls,text)=>({textContent:text}), storageStat:()=>{}, bytes:String});
  vm.runInContext('let currentLab=null, labLoading=false, labError=null;'+
    source.slice(source.indexOf('async function pollLab()'),source.indexOf('async function copyLabCommands()')),context);
  return {context,nodes};
}
test('failed lab refresh clears old totals, recovery and commands',async () => {
  const {context,nodes}=labContext(async()=>{throw new Error('offline');});
  await context.pollLab();
  for (const [name,node] of nodes) assert.ok(!node.textContent.includes('OLD_ELIGIBLE_RESULT'),name);
  assert.ok(nodes.get('recovery').textContent.includes('offline'));
  assert.ok(nodes.get('commands').textContent.includes('Niciun plan'));
});
test('malformed/write-enabled lab response is refused and concurrent refreshes share one request',async () => {
  let requests=0, release;
  const gate=new Promise(resolve=>{release=resolve;});
  const {context,nodes}=labContext(async()=>{requests++; await gate; return {ok:true,json:async()=>({mode:'read-only',execution_enabled:true,reasons:[],recovery:{automatic_repairs:false}})};});
  const first=context.pollLab();
  const second=context.pollLab();
  assert.equal(requests,1);
  release(); await Promise.all([first,second]);
  assert.ok(nodes.get('reasons').textContent.includes('invalidă'));
});
