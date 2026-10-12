// Actual native Tauri + independent Python service; no model or cloud account.
import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import net from 'node:net';

const root=resolve('../..'),directory=resolve(root,'.cache',`native-external-${Date.now()}`);
mkdirSync(directory,{recursive:true});
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
const agentPort=await freePort(),debugPort=await freePort();
const sample=spawn('python',[resolve(root,'examples/external_agent_server.py'),'--port',String(agentPort)],{windowsHide:true});
const desktop=spawn(resolve(root,'target/debug/orchestration-desktop.exe'),[],{windowsHide:true,env:{...process.env,
 ORCHESTRATION_LAB_DATA_DIR:resolve(directory,'workspace'),ORCHESTRATION_LAB_TEST_HIDDEN:'1',
 WEBVIEW2_USER_DATA_FOLDER:resolve(directory,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${debugPort}`}});
let diagnostics='';for(const child of [sample,desktop])child.stderr.on('data',d=>diagnostics+=d);
let browser;
try{
 for(let i=0;i<40;i++){
  if(desktop.exitCode!==null||sample.exitCode!==null)throw new Error(`Process exited: ${diagnostics}`);
  try{await fetch(`http://127.0.0.1:${agentPort}/.well-known/agent-card.json`);browser=await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`,{timeout:1000});break;}catch{await new Promise(r=>setTimeout(r,500));}
 }
 if(!browser)throw new Error(`No native transport: ${diagnostics}`);
 let page;
 for(let i=0;i<30;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri'));if(page)break;await new Promise(r=>setTimeout(r,300));}
 if(!page)throw new Error('No native webview');
 await page.getByRole('heading',{name:'Give your agents a place to work.'}).waitFor({timeout:20000});
 const rpc=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
 const command=command=>rpc('command',{command});
 await command({type:'import_document',name:'external-notes.md',text:'SAMPLE42: Orchard compares FIFO and priority aging. Synthetic source for external integration.'});
 const initial=(await rpc('snapshot')).workspace;
 const settings={...initial.settings,mode:'ollama',worker_slots:1};
 await command({type:'save_settings',settings});
 const evidence=[];
 for(const protocol of ['http_json','a2a_json_rpc']){
  for(const [id,name] of [['external-reader','External Reader'],['external-writer','External Writer']]){
   await command({type:'save_agent',agent:{...initial.agents[0],id,name,model:'',read_permission:'ask',external:{protocol,
    endpoint:`http://127.0.0.1:${agentPort}/${protocol==='http_json'?'run':'a2a'}`,timeout_seconds:60,bearer_token_env:null,send_context:true}}});
  }
  const task=(id,agent_id,dependencies,publish_report,document_id)=>({id,name:id,agent_id,dependencies,publish_report,document_id,steps:1,fail_once:false,release_after_ms:0,resource_plan:[]});
  await command({type:'save_workflow',workflow:{id:`native-${protocol}`,name:`Native external ${protocol}`,tasks:[
   task('research','external-reader',[],false,initial.documents[0].id),task('report','external-writer',['research'],true,null)]}});
  await command({type:'launch',workflow_id:`native-${protocol}`,prompt:'Use the source notes and summarize the comparison.'});
  await page.locator('nav [data-nav="approvals"]').click();
  await page.getByRole('button',{name:'Allow this document'}).waitFor({timeout:10000});
  await page.getByRole('button',{name:'Allow this document'}).click();
  await page.getByRole('button',{name:'Allow this report'}).waitFor({timeout:15000});
  await page.getByRole('button',{name:'Allow this report'}).click();
  let run;
  for(let i=0;i<40;i++){run=(await rpc('snapshot')).workspace.runs.at(-1);if(['completed','failed','cancelled'].includes(run.status))break;await new Promise(r=>setTimeout(r,250));}
  if(run.status!=='completed'||run.api_calls!==2||run.tool_calls!==1||run.artifacts.length!==1||!run.tasks[1].output.includes('SAMPLE42'))throw new Error(`External flow failed: ${JSON.stringify(run.tasks.map(t=>({status:t.status,reason:t.wait_reason})))}`);
  const exported=await rpc('export_run',{runId:run.id});
  evidence.push({protocol,status:run.status,agent_calls:run.api_calls,document_reads:run.tool_calls,reports:run.artifacts.length,export:exported});
 }
 await page.locator('nav [data-nav="agents"]').click();
 await page.locator('[data-edit-agent="external-reader"]').click();
 await page.getByLabel('Agent connection',{exact:true}).waitFor();
 mkdirSync(resolve(root,'doc/screenshots'),{recursive:true});
 await page.screenshot({path:resolve(root,'doc/screenshots/native-external-agent.png'),fullPage:true});
 writeFileSync(resolve(root,'doc/screenshots/native-external-check.json'),JSON.stringify({checked_on:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Colombo'}),transport:'Actual Tauri commands / Windows WebView2 / independent Python sample service',result:'passed',evidence,scope:'Synthetic external service, not a cloud provider or full A2A compliance test'},null,2)+'\n');
 console.log('PASS: native HTTP and A2A external-agent workflows, actual approved file read, dependency context, report approval and exports.');
}finally{await browser?.close();desktop.kill();sample.kill();}
