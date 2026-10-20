// Actual Tauri/WebView2 transport check. Launches a hidden window with isolated data.
import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';
import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import net from 'node:net';
const root=resolve('../..');
const dir=resolve(root,'.cache',`native-${Date.now()}`);mkdirSync(dir,{recursive:true});
const server=net.createServer();await new Promise(r=>server.listen(0,'127.0.0.1',r));
const port=server.address().port;await new Promise(r=>server.close(r));
const child=spawn(resolve(root,'target/debug/orchestration-desktop.exe'),[],{
  windowsHide:true,env:{...process.env,
    ORCHESTRATION_LAB_DATA_DIR:resolve(dir,'workspace'),ORCHESTRATION_LAB_TEST_HIDDEN:'1',
    WEBVIEW2_USER_DATA_FOLDER:resolve(dir,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}
});
let diagnostics='';child.stderr.on('data',d=>diagnostics+=d);child.stdout.on('data',d=>diagnostics+=d);
let browser;
try{
  for(let i=0;i<40;i++){
    if(child.exitCode!==null)throw new Error(`Desktop exited ${child.exitCode}: ${diagnostics}`);
    try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});break;}catch{await new Promise(r=>setTimeout(r,500));}
  }
  if(!browser)throw new Error(`No WebView2 debugger: ${diagnostics}`);
  let page;
  for(let i=0;i<30;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri'));if(page)break;await new Promise(r=>setTimeout(r,300));}
  if(!page)throw new Error('No Tauri webview page');
  await page.getByRole('button',{name:'Continue offline'}).click({timeout:20000});await page.locator('nav [data-nav="overview"]').click();await page.getByRole('heading',{name:'Give your agents a place to work.'}).waitFor({timeout:20000});
  await page.getByRole('button',{name:'Start workflow'}).click();
  await page.getByRole('heading',{name:'Task progress'}).waitFor({timeout:10000});
  await page.locator('nav [data-nav="approvals"]').click();
  await page.getByRole('button',{name:'Allow this report'}).waitFor({timeout:30000});
  await page.getByRole('button',{name:'Allow this report'}).click();
  await page.locator('nav [data-nav="runs"]').click();
  await page.locator('td .badge.completed').waitFor({timeout:15000});
  await page.getByRole('button',{name:'Open ↗',exact:true}).click();
  await page.getByRole('button',{name:'Export JSON'}).click();
  await page.getByRole('status').filter({hasText:'Saved:'}).waitFor();
  await page.locator('nav [data-nav="documents"]').click();
  await page.getByLabel('Text or Markdown file').setInputFiles({name:'native-notes.md',mimeType:'text/markdown',buffer:Buffer.from('NATIVE_DOCUMENT_INPUT: approved research notes.')});
  await page.getByRole('button',{name:'Add document',exact:true}).click();
  await page.getByRole('cell',{name:'native-notes.md',exact:false}).waitFor();
  await page.locator('nav [data-nav="agents"]').click();
  await page.locator('[data-edit-agent="researcher"]').click();
  await page.getByLabel('Permission to read selected documents').selectOption('ask');
  await page.getByRole('button',{name:'Save agent',exact:true}).click();
  await page.getByRole('dialog').waitFor({state:'hidden'});
  await page.locator('nav [data-nav="workflows"]').click();
  await page.locator('[data-workflow-card="parallel-review"]').click();await page.locator('[data-edit-workflow="parallel-review"]').click();
  await page.locator('[name="document-0"]').selectOption({label:'native-notes.md'});
  await page.getByRole('button',{name:'Save workflow',exact:true}).click();
  await page.getByRole('dialog').waitFor({state:'hidden'});
  await page.locator('nav [data-nav="overview"]').click();
  await page.getByLabel('Workflow template').selectOption('parallel-review');
  await page.getByRole('button',{name:'Start workflow'}).click();
  await page.locator('nav [data-nav="approvals"]').click();
  await page.getByRole('button',{name:'Allow this document'}).waitFor({timeout:15000});
  await page.getByRole('button',{name:'Allow this document'}).click();
  await page.locator('nav [data-nav="runs"]').click();
  await page.locator('td .badge.completed').nth(1).waitFor({timeout:15000});
  await page.getByRole('button',{name:'Open ↗',exact:true}).first().click();
  await page.getByRole('button',{name:'Export JSON'}).click();
  await page.getByRole('status').filter({hasText:/run-.*\.json/}).waitFor();
  const documentRunPath=(await page.getByRole('status').textContent()).replace('Saved: ','');
  const documentRun=JSON.parse(readFileSync(documentRunPath,'utf8'));
  if(documentRun.tool_calls!==1||documentRun.api_calls!==9||!documentRun.memory.some(e=>e.kind==='document'&&e.visibility==='private'&&e.text.includes('NATIVE_DOCUMENT_INPUT')))
    throw new Error('Native document approval/read/context contract failed');
  await page.locator('nav [data-nav="memory"]').click();
  await page.getByLabel('Note',{exact:true}).fill('Native supervisor note.');
  await page.getByRole('button',{name:'Save memory note',exact:true}).click();
  await page.locator('.memory-card').filter({hasText:'Native supervisor note.'}).waitFor();
  await page.getByLabel('Search saved context').fill('Native supervisor note.');
  if(await page.locator('.memory-card:visible').count()!==1)throw new Error('Native memory search failed');
  await page.locator('nav [data-nav="research"]').click();
  await page.getByLabel('Trials',{exact:true}).fill('2');
  await page.getByRole('button',{name:'Run comparison',exact:true}).click();
  await page.getByRole('heading',{name:'Where tasks waited'}).waitFor({timeout:15000});
  if(await page.locator('.experiment').count()!==4)throw new Error('Expected four policy summaries');
  await page.getByRole('button',{name:'Export JSON + CSV'}).click();
  await page.getByRole('status').filter({hasText:'scheduler-experiment-42-'}).waitFor();
  const csvPath=(await page.getByRole('status').textContent()).replace('Saved: ','');
  const report=JSON.parse(readFileSync(csvPath.replace(/\.csv$/,'.json'),'utf8'));
  if(report.summaries.length!==4||report.trials.length!==8)throw new Error('Unexpected experiment export');
  await page.getByRole('button',{name:'Run 3-agent deadlock',exact:true}).click();
  await page.getByRole('heading',{name:'Cycle confirmed · cancelled task-2'}).waitFor({timeout:20000});
  await page.locator('.run-bar > .badge.failed').waitFor({timeout:15000});
  await page.getByRole('button',{name:'Export JSON'}).click();
  await page.getByRole('status').filter({hasText:/run-.*\.json/}).waitFor();
  const runPath=(await page.getByRole('status').textContent()).replace('Saved: ','');
  const run=JSON.parse(readFileSync(runPath,'utf8'));
  if(run.deadlock_recoveries.length!==1||run.tasks.filter(t=>t.status==='completed').length!==2||Object.keys(run.resource_owners).length)
    throw new Error('Deadlock recovery did not release resources and finish surviving tasks');
  mkdirSync(resolve(root,'doc/screenshots'),{recursive:true});
  await page.screenshot({path:resolve(root,'doc/screenshots/native-desktop.png'),fullPage:true});
  writeFileSync(resolve(root,'doc/screenshots/native-check.json'),JSON.stringify({checked_on:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Colombo'}),transport:'Tauri commands in Windows WebView2',result:'Passed: workflow, report export, document import/approval/read, private memory, user note/search, four-policy comparison/export, three-agent deadlock recovery and survivor progress',documents:{tool_attempts:documentRun.tool_calls,model_attempts:documentRun.api_calls,private_input:true},policies:report.summaries.map(s=>s.policy),deadlock:{witness_edges:run.deadlock_recoveries[0].witness.edges.length,victim:run.deadlock_recoveries[0].victim,completed_survivors:2},url:page.url()},null,2)+'\n');
  console.log('PASS: native Tauri/WebView2 document approval/read, memory notes/search, workflow, seeded comparison/export, deadlock recovery and survivor progress.');
}finally{await browser?.close();child.kill();}
