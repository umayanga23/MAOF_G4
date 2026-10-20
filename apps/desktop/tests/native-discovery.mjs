// Actual Tauri discovery/review/registration and a protected synthetic service.
import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';
import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
const root=resolve('../..'),directory=resolve(root,'.cache',`native-discovery-${Date.now()}`);
mkdirSync(directory,{recursive:true});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
const agentPort=await freePort(),debugPort=await freePort();
const env={...process.env,ORCHESTRATION_AGENT_SAMPLE_TOKEN:'SYNTHETIC_NATIVE_DISCOVERY_TOKEN'};
const sample=spawn('python',[resolve(root,'examples/external_agent_server.py'),'--port',String(agentPort),'--require-token'],{windowsHide:true,env});
const desktop=spawn(resolve(root,'target/debug/orchestration-desktop.exe'),[],{windowsHide:true,env:{...env,
 ORCHESTRATION_LAB_DATA_DIR:resolve(directory,'workspace'),ORCHESTRATION_LAB_TEST_HIDDEN:'1',
 WEBVIEW2_USER_DATA_FOLDER:resolve(directory,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${debugPort}`}});
let diagnostics='',browser;for(const child of [sample,desktop])child.stderr.on('data',d=>diagnostics+=d);
try{
 for(let i=0;i<50;i++){
  if(desktop.exitCode!==null||sample.exitCode!==null)throw new Error(`Test process exited: ${diagnostics}`);
  try{await fetch(`http://127.0.0.1:${agentPort}/.well-known/agent-card.json`);browser=await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`,{timeout:1000});break;}catch{await sleep(400);}
 }
 if(!browser)throw new Error(`No native transport: ${diagnostics}`);
 let page;for(let i=0;i<30;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri'));if(page)break;await sleep(300);}
 if(!page)throw new Error('No native webview');
 await page.getByRole('button',{name:'Continue offline'}).click({timeout:20000});await page.locator('nav [data-nav="overview"]').click();await page.getByRole('heading',{name:'Give your agents a place to work.'}).waitFor({timeout:20000});
 const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
 const initial=(await invoke('snapshot')).workspace;
 await page.locator('nav [data-nav="agents"]').click();await page.getByRole('button',{name:'Discover agent',exact:true}).click();
 await page.getByLabel('Agent Card URL',{exact:true}).fill(`http://127.0.0.1:${agentPort}/.well-known/agent-card.json`);
 await page.getByRole('button',{name:'Fetch Agent Card',exact:true}).click();
 await page.getByRole('button',{name:'Use these details',exact:true}).waitFor({timeout:10000});
 assert.equal((await invoke('snapshot')).workspace.agents.length,initial.agents.length);
 assert.match(await page.locator('.discovery-preview').textContent(),/Authentication.*Required by the card/s);
 mkdirSync(resolve(root,'doc/screenshots'),{recursive:true});
 await page.screenshot({path:resolve(root,'doc/screenshots/native-agent-discovery.png'),fullPage:true});
 await page.getByRole('button',{name:'Use these details',exact:true}).click();
 assert.equal(await page.getByLabel('Bearer token environment variable (optional)',{exact:true}).inputValue(),'');
 assert.equal(await page.getByLabel('Permission to save a local report').inputValue(),'deny');
 assert.equal(await page.getByLabel('Permission to read selected documents').inputValue(),'deny');
 assert.equal(await page.getByLabel('Send task context to this external service').isChecked(),false);
 await page.getByLabel('Bearer token environment variable (optional)',{exact:true}).fill('ORCHESTRATION_AGENT_SAMPLE_TOKEN');
 await page.getByRole('button',{name:'Save agent',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
 const registered=(await invoke('snapshot')).workspace.agents.at(-1);assert.equal(registered.discovery.skills[0].id,'research');
 const persisted=readFileSync(resolve(directory,'workspace/workspace.json'),'utf8');assert(!persisted.includes(env.ORCHESTRATION_AGENT_SAMPLE_TOKEN));
 await invoke('command',{command:{type:'save_settings',settings:{...initial.settings,mode:'ollama'}}});
 await invoke('command',{command:{type:'save_workflow',workflow:{id:'native-discovery',name:'Discovered service check',tasks:[{id:'task',name:'Use discovered researcher',agent_id:registered.id,dependencies:[],steps:1,publish_report:false,fail_once:false,document_id:null,release_after_ms:0,resource_plan:[]}]}}});
 await page.locator('nav [data-nav="overview"]').click();await page.getByLabel('Workflow template').selectOption('native-discovery');await page.getByRole('button',{name:'Start workflow'}).click();
 let run;for(let i=0;i<50;i++){run=(await invoke('snapshot')).workspace.runs.at(-1);if(['completed','failed','cancelled'].includes(run.status))break;await sleep(200);}
 assert.equal(run.status,'completed');assert.equal(run.api_calls,1);assert.match(run.tasks[0].output,/EXTERNAL SAMPLE AGENT/);
 assert.equal(run.tasks[0].agent.discovery.card_url,registered.discovery.card_url);
 writeFileSync(resolve(root,'doc/screenshots/native-discovery-check.json'),JSON.stringify({checked_on:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Colombo'}),
  transport:'Actual Tauri / WebView2 / independent Python service',result:'passed',evidence:{preview_does_not_register:true,reviewed_skills:registered.discovery.skills.map(s=>s.id),safe_defaults:true,
   execution_credentials_configured_separately:true,token_value_not_saved:true,pinned_card:true,run_status:run.status,agent_calls:1},scope:'Synthetic service; advertised skills are not verified abilities or full A2A conformance'},null,2)+'\n');
 console.log('PASS: native Agent Card fetch/review, explicit registration, safe defaults and authenticated A2A workflow.');
}finally{await browser?.close();desktop.kill();sample.kill();}
