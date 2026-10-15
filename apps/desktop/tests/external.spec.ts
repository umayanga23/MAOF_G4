import {test,expect} from '@playwright/test';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {createServer} from 'node:http';
import {resolve} from 'node:path';
import {readFileSync} from 'node:fs';

for(const protocol of ['http_json','a2a_json_rpc'] as const){
 test(`external ${protocol}: register, delegate, share results, approve and persist`,async({page})=>{
  const received:any[]=[];const tasks=new Map<string,string>();
  const server=createServer(async(req,res)=>{
   let input='';for await(const chunk of req)input+=chunk;
   const body=JSON.parse(input);received.push(body);
   res.setHeader('Content-Type','application/json');
   if(protocol==='http_json'){
    res.end(JSON.stringify({request_id:body.request_id,output:body.context?`EXTERNAL FINAL: ${body.context}`:'EXTERNAL FINDINGS SAMPLE42',usage:{tokens:0}}));
   }else{
    let result;
    if(body.method==='message/send'){
     const id=`remote-${tasks.size}`;const text=body.params.message.parts[0].text;
     tasks.set(id,text.includes('EXTERNAL FINDINGS SAMPLE42')?`EXTERNAL FINAL: ${text}`:'EXTERNAL FINDINGS SAMPLE42');
     result={kind:'task',id,contextId:id,status:{state:'working'}};
    }else{
     const id=body.params.id;result={kind:'task',id,contextId:id,status:{state:'completed'},artifacts:[{parts:[{kind:'text',text:tasks.get(id)}]}],metadata:{orchestration_usage:{tokens:0}}};
    }
    res.end(JSON.stringify({jsonrpc:'2.0',id:body.id,result}));
   }
  });
  await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));
  const port=(server.address() as any).port;
  const directory=resolve('../..','.cache',`external-ui-${protocol}-${Date.now()}`);
  const child=spawn(resolve('../..','target/debug/examples/ui_bridge.exe'),[directory],{windowsHide:true});
  let seq=0,closing=false,stderr='';const pending=new Map<number,{resolve:(value:any)=>void,reject:(error:Error)=>void}>();
  child.stderr.on('data',d=>stderr+=d.toString());
  const rejectPending=(error:Error)=>{for(const p of pending.values())p.reject(error);pending.clear();};
  child.stdin.on('error',rejectPending);
  child.on('error',rejectPending);
  child.on('exit',code=>rejectPending(new Error(`Bridge exited ${code}: ${stderr}`)));
  createInterface({input:child.stdout}).on('line',line=>{const r=JSON.parse(line);const p=pending.get(r.id);pending.delete(r.id);r.error?p?.reject(new Error(r.error)):p?.resolve(r.value);});
  const rpc=(cmd:string,args:any={})=>new Promise<any>((resolve,reject)=>{
   if(closing||child.exitCode!==null||child.stdin.destroyed){reject(new Error(`Bridge unavailable: ${stderr}`));return;}
   const id=++seq;pending.set(id,{resolve,reject});child.stdin.write(JSON.stringify({id,cmd,args})+'\n',error=>{if(error){pending.delete(id);reject(error);}});
  });
  try{
   await page.exposeFunction('rustInvoke',rpc);
   await page.addInitScript(()=>{const w=window as any;w.isTauri=true;w.__TAURI_INTERNALS__={invoke:(cmd:string,args:any)=>w.rustInvoke(cmd,args)};});
   await page.goto('/');
   await page.getByRole('button',{name:'Continue offline'}).click();await page.locator('nav [data-nav="overview"]').click();await expect(page.getByRole('heading',{name:'Give your agents a place to work.'})).toBeVisible();
   await page.locator('nav [data-nav="agents"]').click();
   const identities:string[]=[];
   for(const name of ['Remote Researcher','Remote Writer']){
    await page.getByRole('button',{name:'Add agent',exact:true}).click();
    await page.getByLabel('Name',{exact:true}).fill(name);
    await page.getByLabel('Role',{exact:true}).fill('External test agent');
    await page.getByLabel('Agent connection',{exact:true}).selectOption(protocol);
    await page.getByLabel('External agent execution URL').fill(`http://127.0.0.1:${port}/execute`);
    await page.getByLabel('Send task context to this external service').check();
    await page.getByRole('button',{name:'Save agent',exact:true}).click();
    await expect(page.getByRole('dialog')).not.toBeVisible();
    identities.push((await rpc('snapshot')).workspace.agents.at(-1).id);
   }
   await page.locator('nav [data-nav="workflows"]').click();
   await page.getByRole('button',{name:'Start blank',exact:true}).click();
   await page.getByLabel('Workflow name',{exact:true}).fill('External team test');
   await page.locator('[name="id-0"]').fill('research');
   await page.locator('[name="agent-0"]').selectOption(identities[0]);
   await page.locator('[name="steps-0"]').fill('1');
   await page.getByRole('button',{name:'Add task',exact:true}).click();
   await page.locator('[name="id-1"]').fill('report');
   await page.locator('[name="agent-1"]').selectOption(identities[1]);
   await page.locator('[name="steps-1"]').fill('1');
   await page.locator('[name="deps-1"]').fill('research');
   await page.locator('[name="report-1"]').check();
   await page.getByRole('button',{name:'Save workflow',exact:true}).click();
   await expect(page.getByRole('dialog')).not.toBeVisible();
   const snapshot=await rpc('snapshot');
   await rpc('command',{command:{type:'save_settings',settings:{...snapshot.workspace.settings,mode:'ollama',worker_slots:1}}});
   await page.locator('nav [data-nav="overview"]').click();
   await page.getByLabel('Workflow template').selectOption(snapshot.workspace.workflows.at(-1).id);
   await page.getByLabel('What should the team work on?').fill('Use supplied SAMPLE42 notes.');
   await page.getByRole('button',{name:'Start workflow'}).click();
   await page.locator('nav [data-nav="approvals"]').click();
   await expect(page.getByRole('button',{name:'Allow this report'})).toBeVisible({timeout:20000});
   await expect(page.locator('.report-preview')).toContainText('EXTERNAL FINDINGS SAMPLE42');
   await page.getByRole('button',{name:'Allow this report'}).click();
   await expect.poll(async()=>(await rpc('snapshot')).workspace.runs.at(-1).status).toBe('completed');
   const run=(await rpc('snapshot')).workspace.runs.at(-1);
   expect(run.api_calls).toBe(2);expect(run.artifacts).toHaveLength(1);
   expect(run.tasks[1].output).toContain('EXTERNAL FINDINGS SAMPLE42');
   expect(run.tasks.every((t:any)=>t.agent.external.protocol===protocol)).toBe(true);
   const exportPath=await rpc('export_run',{runId:run.id});
   expect(JSON.parse(readFileSync(exportPath,'utf8')).status).toBe('completed');
   expect(received.length).toBe(protocol==='http_json'?2:4);
  }finally{
   // Close the polling page before its Rust transport; report real bridge exits
   // through RPC rejection rather than an unhandled stream EPIPE event.
   closing=true;await page.close();
   if(child.exitCode===null){const exited=new Promise<void>(r=>child.once('exit',()=>r()));child.kill();await exited;}
   server.closeAllConnections();await new Promise<void>(r=>server.close(()=>r()));
  }
 });
}
