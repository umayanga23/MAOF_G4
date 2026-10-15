// Native Windows restart/cancel check against an independent A2A service.
import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';
import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import http from 'node:http';
import net from 'node:net';
import assert from 'node:assert/strict';

const root=resolve('../..'),directory=resolve(root,'.cache',`native-remote-${Date.now()}`);
mkdirSync(directory,{recursive:true});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
const calls=[],tasks=new Map();let finished=false,rejectCancel=false;
const server=http.createServer(async(req,res)=>{
 let body='';for await(const chunk of req)body+=chunk;
 const request=JSON.parse(body);calls.push(request.method);
 let task,error;
 if(request.method==='message/send'){
  task={id:`remote-${tasks.size+1}`,canceled:false};tasks.set(task.id,task);
 }else{
  task=tasks.get(request.params.id);
  if(!task)error={code:-32001,message:'Task not found'};
  else if(request.method==='tasks/cancel'){
   if(rejectCancel)error={code:-32002,message:'Cancellation unsupported'};
   else if(!finished)task.canceled=true;
  }
 }
 const state=task?.canceled?'canceled':finished?'completed':'working';
 res.setHeader('Content-Type','application/json');
 res.end(JSON.stringify({jsonrpc:'2.0',id:request.id,...(error?{error}:{result:{kind:'task',id:task.id,status:{state},
  artifacts:state==='completed'?[{artifactId:'answer',parts:[{kind:'text',text:'REMOTE_RESULT: checked once after reconnect.'}]}]:[],
  metadata:{orchestration_usage:{tokens:37}}}})}));
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const endpoint=`http://127.0.0.1:${server.address().port}/a2a`;
let desktop,browser,page,diagnostics='',launchNumber=0;
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const command=command=>invoke('command',{command});
const snapshot=async()=> (await invoke('snapshot')).workspace;
async function waitRun(predicate){for(let i=0;i<100;i++){const run=(await snapshot()).runs.at(-1);if(run&&predicate(run))return run;await sleep(150);}throw new Error('Remote run did not reach expected state');}
async function openRun(run){await page.locator('nav [data-nav="runs"]').click();await page.locator(`[data-open-run="${run.id}"]`).click();await page.getByRole('heading',{name:'External task status',exact:true}).waitFor();}
async function startDesktop(){
 const port=await freePort();launchNumber++;
 desktop=spawn(resolve(root,'target/debug/orchestration-desktop.exe'),[],{windowsHide:true,env:{...process.env,
  ORCHESTRATION_LAB_DATA_DIR:resolve(directory,'workspace'),ORCHESTRATION_LAB_TEST_HIDDEN:'1',
  WEBVIEW2_USER_DATA_FOLDER:resolve(directory,`webview-${launchNumber}`),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
 desktop.stderr.on('data',d=>diagnostics+=d);desktop.on('error',e=>diagnostics+=e);
 for(let i=0;i<50;i++){
  if(desktop.exitCode!==null)throw new Error(`Test desktop exited: ${diagnostics}`);
  try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});break;}catch{await sleep(400);}
 }
 if(!browser)throw new Error(`No native debugger: ${diagnostics}`);
 for(let i=0;i<30;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri'));if(page)break;await sleep(300);}
 if(!page)throw new Error('No native webview');
 await page.getByRole('button',{name:'Continue offline'}).click({timeout:20000});await page.locator('nav [data-nav="overview"]').click();await page.getByRole('heading',{name:'Give your agents a place to work.'}).waitFor({timeout:20000});
}
async function stopTestDesktop(){
 await browser?.close();browser=undefined;page=undefined;
 if(desktop&&desktop.exitCode===null){const exited=new Promise(r=>desktop.once('exit',r));desktop.kill();await exited;}
 desktop=undefined;
}
async function launch(){
 const previous=(await snapshot()).runs.at(-1)?.id;
 await page.locator('nav [data-nav="overview"]').click();await page.getByLabel('Workflow template').selectOption('remote-check');
 await page.getByRole('button',{name:'Start workflow'}).click();
 // A click does not await the app's asynchronous durable command. The previous
 // run also has a remote ID, so wait for a NEW identity before opening it.
 return waitRun(r=>r.id!==previous&&!!r.tasks[0].remote?.task_id);
}
const evidence=[];
try{
 await startDesktop();const initial=await snapshot();
 await command({type:'save_settings',settings:{...initial.settings,mode:'ollama',worker_slots:1,api_budget:1,step_tokens:128,output_tokens:32}});
 const agent={...initial.agents[0],id:'remote-check',name:'Remote Check',model:'',external:{protocol:'a2a_json_rpc',endpoint,timeout_seconds:30,bearer_token_env:null,send_context:false}};
 await command({type:'save_agent',agent});
 await command({type:'save_workflow',workflow:{id:'remote-check',name:'Remote lifecycle check',tasks:[{id:'task',name:'Check external result',agent_id:agent.id,dependencies:[],publish_report:false,document_id:null,steps:1,fail_once:false,release_after_ms:0,resource_plan:[]}]}});
 let run=await launch();await openRun(run);
 await page.locator('.remote-panel').filter({hasText:run.tasks[0].remote.task_id}).waitFor();
 const persisted=JSON.parse(readFileSync(resolve(directory,'workspace/workspace.json'),'utf8'));
 assert.equal(persisted.runs.at(-1).tasks[0].remote.task_id,run.tasks[0].remote.task_id);
 const originalId=run.tasks[0].remote.task_id;
 await stopTestDesktop();finished=true;await startDesktop();
 run=(await snapshot()).runs.at(-1);assert.equal(run.status,'interrupted');await openRun(run);
 await page.getByRole('button',{name:'Recover checkpoint',exact:true}).click();
 await page.getByRole('button',{name:'Resume',exact:true}).click();
 run=await waitRun(r=>r.status==='completed');
 assert.equal(run.tasks[0].remote.task_id,originalId);assert.equal(calls.filter(m=>m==='message/send').length,1);
 assert.equal(run.api_calls,1);assert.equal(run.tasks[0].tokens,37);assert.equal(run.tasks[0].uncertain_tokens,0);
 assert.equal(run.tasks[0].completed_steps,1);assert.match(run.tasks[0].output,/REMOTE_RESULT/);
 evidence.push({scenario:'Native process restart, saved ID, recover/resume',status:run.status,remote_id:originalId,submissions:1,agent_calls:run.api_calls,tokens:37,uncertain:0});
 finished=false;run=await launch();await openRun(run);
 await page.getByRole('button',{name:'Stop run',exact:true}).click();
 run=await waitRun(r=>r.status==='cancelled'&&!r.tasks[0].remote.pending);
 assert.equal(run.tasks[0].remote.cancellation,'confirmed');assert.equal(run.tasks[0].completed_steps,0);assert.equal(run.tasks[0].output,'');
 evidence.push({scenario:'Confirmed remote cancellation',status:run.status,cancellation:run.tasks[0].remote.cancellation,accepted_steps:0});
 rejectCancel=true;await command({type:'save_agent',agent:{...agent,external:{...agent.external,timeout_seconds:3}}});
 run=await launch();await openRun(run);await page.getByRole('button',{name:'Stop run',exact:true}).click();
 run=await waitRun(r=>r.status==='cancelled'&&r.tasks[0].remote.pending);
 assert.equal(run.tasks[0].remote.cancellation,'unconfirmed');assert.equal(run.tasks[0].uncertain_tokens,128);
 const cancels=calls.filter(m=>m==='tasks/cancel').length,submissions=calls.filter(m=>m==='message/send').length;
 await page.screenshot({path:resolve(root,'doc/screenshots/native-remote-status.png'),fullPage:true});
 finished=true;await page.getByRole('button',{name:'Check remote status',exact:true}).click();
 run=await waitRun(r=>r.status==='cancelled'&&!r.tasks[0].remote.pending);
 assert.equal(calls.filter(m=>m==='tasks/cancel').length,cancels);assert.equal(calls.filter(m=>m==='message/send').length,submissions);
 assert.equal(run.tasks[0].tokens,37);assert.equal(run.tasks[0].uncertain_tokens,0);assert.equal(run.tasks[0].completed_steps,0);assert.equal(run.tasks[0].output,'');
 evidence.push({scenario:'Rejected cancel, manual status reconciliation',status:run.status,cancellation:run.tasks[0].remote.cancellation,tokens:37,uncertain:0,accepted_steps:0,repeated_submission:false,repeated_cancel:false});
 writeFileSync(resolve(root,'doc/screenshots/native-remote-check.json'),JSON.stringify({checked_on:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Colombo'}),transport:'Actual Tauri / WebView2 / independent Node A2A fixture',result:'passed',evidence,scope:'Synthetic service, not full A2A conformance or a cloud provider test'},null,2)+'\n');
 console.log('PASS: native restart/status recovery, confirmed cancel and rejected-cancel reconciliation without replay.');
}finally{await stopTestDesktop();server.closeAllConnections();await new Promise(r=>server.close(r));}
