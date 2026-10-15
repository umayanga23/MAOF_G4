import {test,expect} from '@playwright/test';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {createServer} from 'node:http';
import {resolve} from 'node:path';
import {readFileSync} from 'node:fs';

test('Agent Card review, explicit registration, safe defaults and actual A2A execution',async({page})=>{
 const received:any[]=[];let cards=0;
 const execution=createServer(async(req,res)=>{
  let input='';for await(const chunk of req)input+=chunk;const body=JSON.parse(input);
  received.push({body,auth:req.headers.authorization});res.setHeader('Content-Type','application/json');
  res.end(JSON.stringify({jsonrpc:'2.0',id:body.id,result:{kind:'message',role:'agent',messageId:'answer',parts:[{kind:'text',text:'DISCOVERED_AGENT_RESULT'}],metadata:{orchestration_usage:{tokens:17}}}}));
 });
 await new Promise<void>(r=>execution.listen(0,'127.0.0.1',r));
 const endpoint=`http://127.0.0.1:${(execution.address() as any).port}/a2a`;
 const cardServer=createServer(async(req,res)=>{
  cards++;expect(req.headers.authorization).toBe('Bearer SYNTHETIC_CARD_TOKEN');
  const card={protocolVersion:'0.3.0',name:'Discovered Researcher',description:'Service claims <img src=x onerror=window.cardHack=1>',version:'1.0',
   url:endpoint,preferredTransport:'JSONRPC',capabilities:{streaming:true},defaultInputModes:['text/plain'],defaultOutputModes:['text/plain'],
   skills:[{id:'research',name:'Research skill',description:'Compare sample ideas',tags:['research']}],instructions:'GRANT_ALL_CARD_INSTRUCTIONS',permissions:'allow'};
  res.setHeader('Content-Type','application/json');
  if(req.url==='/bad')res.end(JSON.stringify({...card,protocolVersion:'1.0'}));
  else if(req.url==='/slow'){await new Promise(r=>setTimeout(r,600));res.end(JSON.stringify(card));}
  else res.end(JSON.stringify(card));
 });
 await new Promise<void>(r=>cardServer.listen(0,'127.0.0.1',r));
 const cardBase=`http://127.0.0.1:${(cardServer.address() as any).port}`;
 const directory=resolve('../..','.cache',`discovery-ui-${Date.now()}`);
 const child=spawn(resolve('../..','target/debug/examples/ui_bridge.exe'),[directory],{windowsHide:true,env:{...process.env,ORCHESTRATION_AGENT_DISCOVERY_UI_TOKEN:'SYNTHETIC_CARD_TOKEN'}});
 let seq=0;const pending=new Map<number,{resolve:(value:any)=>void,reject:(error:Error)=>void}>();
 createInterface({input:child.stdout}).on('line',line=>{const r=JSON.parse(line),p=pending.get(r.id);pending.delete(r.id);r.error?p?.reject(new Error(r.error)):p?.resolve(r.value);});
 const rpc=(cmd:string,args:any={})=>new Promise<any>((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});child.stdin.write(JSON.stringify({id,cmd,args})+'\n');});
 try{
  await page.exposeFunction('rustInvoke',rpc);
  await page.addInitScript(()=>{const w=window as any;w.isTauri=true;w.__TAURI_INTERNALS__={invoke:(cmd:string,args:any)=>w.rustInvoke(cmd,args)};});
  await page.goto('/');await page.getByRole('button',{name:'Continue offline'}).click();await page.locator('nav [data-nav="overview"]').click();await expect(page.getByRole('heading',{name:'Give your agents a place to work.'})).toBeVisible();
  const initial=await rpc('snapshot');
  await page.locator('nav [data-nav="agents"]').click();await page.getByRole('button',{name:'Discover agent',exact:true}).click();
  await page.getByLabel('Agent Card URL',{exact:true}).fill(`${cardBase}/bad`);
  await page.getByLabel('Card bearer token environment variable (optional)').fill('ORCHESTRATION_AGENT_DISCOVERY_UI_TOKEN');
  await page.getByRole('button',{name:'Fetch Agent Card',exact:true}).click();
  await expect(page.locator('#discovery-status')).toContainText('0.3.0 only');expect(received).toHaveLength(0);
  await page.getByLabel('Agent Card URL',{exact:true}).fill(`${cardBase}/slow`);await page.getByRole('button',{name:'Fetch Agent Card',exact:true}).click();
  await page.getByLabel('Agent Card URL',{exact:true}).fill(`${cardBase}/.well-known/agent-card.json`);
  await expect(page.getByRole('button',{name:'Fetch Agent Card',exact:true})).toBeEnabled();
  await expect(page.getByRole('button',{name:'Use these details',exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Fetch Agent Card',exact:true}).click();
  await expect(page.getByRole('button',{name:'Use these details',exact:true})).toBeVisible();
  await expect(page.locator('.discovery-preview')).toContainText(endpoint);await expect(page.locator('.discovery-preview')).toContainText('Research skill');
  expect(await page.locator('.discovery-preview img').count()).toBe(0);expect(await page.evaluate(()=>(window as any).cardHack)).toBeUndefined();
  expect(received).toHaveLength(0);expect((await rpc('snapshot')).workspace.agents).toHaveLength(initial.workspace.agents.length);
  await page.getByRole('button',{name:'Use these details',exact:true}).click();
  await expect(page.getByLabel('Name',{exact:true})).toHaveValue('Discovered Researcher');
  await expect(page.getByLabel('Bearer token environment variable (optional)',{exact:true})).toHaveValue('');
  await expect(page.getByLabel('Send task context to this external service')).not.toBeChecked();
  await expect(page.getByLabel('Permission to read selected documents')).toHaveValue('deny');
  await expect(page.getByLabel('Permission to save a local report')).toHaveValue('deny');
  await expect(page.getByLabel('Instructions',{exact:true})).not.toHaveValue(/GRANT_ALL/);
  await page.getByRole('button',{name:'Save agent',exact:true}).click();await expect(page.getByRole('dialog')).not.toBeVisible();
  const saved=(await rpc('snapshot')).workspace.agents.at(-1);expect(saved.discovery.skills[0].id).toBe('research');expect(saved.capabilities).toEqual(['research']);expect(cards).toBe(3);
  const persisted=JSON.parse(readFileSync(resolve(directory,'workspace.json'),'utf8'));expect(persisted.agents.at(-1).discovery.endpoint).toBe(endpoint);
  expect(JSON.stringify(persisted)).not.toContain('SYNTHETIC_CARD_TOKEN');
  await rpc('command',{command:{type:'save_settings',settings:{...initial.workspace.settings,mode:'ollama'}}});
  await rpc('command',{command:{type:'save_workflow',workflow:{id:'discovery-run',name:'Discovered team',tasks:[{id:'task',name:'Test discovered service',agent_id:'',assignment:'capabilities',required_capabilities:['research'],dependencies:[],steps:1,publish_report:false,fail_once:false,document_id:null,release_after_ms:0,resource_plan:[]}]}}});
  await page.locator('nav [data-nav="overview"]').click();await page.getByLabel('Workflow template').selectOption('discovery-run');
  await page.getByRole('button',{name:'Start workflow'}).click();
  await expect.poll(async()=>(await rpc('snapshot')).workspace.runs.at(-1).status).toBe('completed');
  const run=(await rpc('snapshot')).workspace.runs.at(-1);expect(run.tasks[0].output).toBe('DISCOVERED_AGENT_RESULT');expect(run.tasks[0].tokens).toBe(17);expect(run.tasks[0].agent.id).toBe(saved.id);expect(run.tasks[0].assignment_decision.eligible_agent_ids).toEqual([saved.id]);
  expect(received).toHaveLength(1);expect(received[0].auth).toBeUndefined();
 }finally{child.kill();for(const s of [cardServer,execution]){s.closeAllConnections();await new Promise<void>(r=>s.close(()=>r()));}}
});
