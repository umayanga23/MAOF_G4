import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import net from 'node:net';
import {assignmentFlow} from './assignment-flow.mjs';
const root=resolve('../..'),directory=resolve(root,'.cache',`native-assignment-${Date.now()}`);
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
 const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
 mkdirSync(resolve(root,'doc/screenshots'),{recursive:true});
 const evidence=await assignmentFlow(page,invoke,resolve(root,'doc/screenshots/native-agent-assignment.png'));
 writeFileSync(resolve(root,'doc/screenshots/native-assignment-check.json'),JSON.stringify({checked_on:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Colombo'}),
  transport:'Actual Tauri / WebView2 / Rust service',result:'passed',evidence,scope:'Deterministic Demo workflow; no model quality or external interoperability claim'},null,2)+'\n');
 console.log('PASS: native capability forms, atomic no-match, permission filtering, pinned selection, document/report approvals, export and next-run edits.');
}finally{await browser?.close();child.kill();}
