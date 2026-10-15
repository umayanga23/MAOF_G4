// Actual Tauri transport under overload; no production worker-fault switches.
import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
const root=resolve('../..'),directory=resolve(root,'.cache',`native-runtime-${Date.now()}`);
mkdirSync(directory,{recursive:true});
const portServer=net.createServer();await new Promise(r=>portServer.listen(0,'127.0.0.1',r));
const port=portServer.address().port;await new Promise(r=>portServer.close(r));
const child=spawn(resolve(root,'target/debug/orchestration-desktop.exe'),[],{windowsHide:true,env:{...process.env,
 ORCHESTRATION_LAB_DATA_DIR:resolve(directory,'workspace'),ORCHESTRATION_LAB_TEST_HIDDEN:'1',WEBVIEW2_USER_DATA_FOLDER:resolve(directory,'webview'),
 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));let browser,diagnostics='';child.stderr.on('data',d=>diagnostics+=d);
try{
 for(let i=0;i<50;i++){
  if(child.exitCode!==null)throw new Error(`Desktop exited: ${diagnostics}`);
  try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});break;}catch{await sleep(400);}
 }
 if(!browser)throw new Error(`No native transport: ${diagnostics}`);
 let page;for(let i=0;i<30;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri'));if(page)break;await sleep(300);}
 if(!page)throw new Error('No native webview');
 await page.getByRole('button',{name:'Continue offline'}).click({timeout:20000});await page.locator('nav [data-nav="overview"]').click();await page.getByRole('heading',{name:'Give your agents a place to work.'}).waitFor({timeout:20000});
 const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
 const command=command=>invoke('command',{command});
 const initial=await invoke('snapshot');
 const settings={...initial.workspace.settings,mode:'demo',worker_slots:8,memory_mb:1024,demo_delay_ms:3000,api_budget:100};
 await command({type:'save_settings',settings});
 const tasks=[];
 for(let i=0;i<8;i++){
  const agent={...initial.workspace.agents[0],id:`stress-${i}`,name:`Worker ${i+1}`,memory_mb:64,external:null};
  await command({type:'save_agent',agent});
  tasks.push({...initial.workspace.workflows[1].tasks[0],id:`stress-${i}`,name:`Independent task ${i+1}`,agent_id:agent.id,
   assignment:'manual',required_capabilities:[],dependencies:[],steps:1,publish_report:false,fail_once:false,document_id:null});
 }
 await command({type:'save_workflow',workflow:{id:'stress',name:'Bounded runtime check',tasks}});
 await command({type:'launch',workflow_id:'stress',prompt:'Check bounded transport and stop under load.'});
 let before;
 for(let i=0;i<40;i++){before=await invoke('snapshot');if(before.runtime.active_workers===8)break;await sleep(50);}
 assert.equal(before.runtime.active_workers,8);
 const runId=before.workspace.runs.at(-1).id;
 const submittedAt=Date.now();
 const receipts=await page.evaluate(async({settings,runId})=>{
  const invoke=command=>window.__TAURI_INTERNALS__.invoke('command',{command}).then(()=>({ok:true}),error=>({ok:false,error:String(error)}));
  const normal=Array.from({length:128},()=>invoke({type:'save_settings',settings}));
  const stop=invoke({type:'control',run_id:runId,action:'stop'});
  return {normal:await Promise.all(normal),stop:await stop};
 },{settings,runId});
 assert.equal(receipts.stop.ok,true,JSON.stringify(receipts.stop));
 const rejected=receipts.normal.filter(r=>!r.ok);
 assert.ok(rejected.length>0,'Native overload must exercise a full lane');
 assert.ok(rejected.every(r=>r.error.includes('CommandQueueFull')));
 let after;
 for(let i=0;i<100;i++){after=await invoke('snapshot');if(after.workspace.runs.at(-1).status==='cancelled'&&after.runtime.active_workers===0&&after.runtime.results_pending===0)break;await sleep(100);}
 const r=after.workspace.runs.at(-1),h=after.runtime,w=after.writer;
 assert.equal(w.busy,false);assert.equal(w.commits_pending,0);assert.equal(w.exports_pending,0);assert.equal(w.failures,0);assert.equal(w.exports_in_flight,0);assert.ok(w.committed_checkpoints>0);
 assert.equal(r.status,'cancelled');assert.equal(h.active_workers,0);assert.equal(h.results_pending,0);
 assert.equal(h.commands_pending,0);assert.equal(h.controls_pending,0);assert.equal(h.command_bytes_in_flight,0);
 assert.equal(h.rejected_commands,rejected.length);assert.equal(h.worker_failures,0);assert.equal(h.driver_running,true);
 assert.ok(r.tasks.every(t=>t.attempt_id===null&&t.reserved_tokens===0&&t.uncertain_tokens===0&&t.completed_steps===0));
 assert.equal(r.artifacts.length,0);
 await page.locator('nav [data-nav="runs"]').click();await page.locator(`[data-open-run="${runId}"]`).click();
 await page.locator('.runtime-health summary').click();
 await page.locator('.runtime-health[open]').waitFor();
 await sleep(1100);assert.ok(await page.locator('.runtime-health').getAttribute('open')!==null,'Diagnostics remain open across refresh');
 await page.screenshot({path:resolve(root,'doc/screenshots/native-runtime.png'),fullPage:true});
 writeFileSync(resolve(root,'doc/screenshots/native-runtime-check.json'),JSON.stringify({checked_on:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Colombo'}),
  result:'passed',transport:'Actual Tauri / WebView2 / Rust service',normal_submissions:128,normal_accepted:128-rejected.length,
  normal_refused_before_acceptance:rejected.length,stop_accepted:true,elapsed_ms:Date.now()-submittedAt,workers_before:8,health_after:h,writer_after:w,
  final_status:r.status,scope:'Deterministic Demo overload/drain. Worker-fault injections are separate Rust tests; no live inference or latency guarantee.'},null,2)+'\n');
 console.log(`PASS: native bounded overload (${rejected.length} explicit refusals), reserved Stop, eight-worker draining, asynchronous writer and Engine health.`);
}finally{await browser?.close();child.kill();}
