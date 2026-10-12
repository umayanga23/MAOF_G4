import {test,expect} from '@playwright/test';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {resolve} from 'node:path';
import {assignmentFlow} from './assignment-flow.mjs';

test('capability routing through forms preserves admission, approvals and pinned decisions',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 const child=spawn(resolve('../..','target/debug/examples/ui_bridge.exe'),[resolve('../..','.cache',`assignment-ui-${Date.now()}`)],{windowsHide:true});
 let seq=0;const pending=new Map<number,{resolve:(v:any)=>void,reject:(e:Error)=>void}>();
 createInterface({input:child.stdout}).on('line',line=>{const r=JSON.parse(line),p=pending.get(r.id);pending.delete(r.id);r.error?p?.reject(new Error(r.error)):p?.resolve(r.value);});
 child.on('exit',code=>{for(const p of pending.values())p.reject(new Error(`Bridge exited: ${code}`));pending.clear();});
 const invoke=(cmd:string,args:any={})=>new Promise<any>((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});child.stdin.write(JSON.stringify({id,cmd,args})+'\n');});
 try{
  await page.exposeFunction('rustInvoke',invoke);
  await page.addInitScript(()=>{const w=window as any;w.isTauri=true;w.__TAURI_INTERNALS__={invoke:(cmd:string,args:any)=>w.rustInvoke(cmd,args)};});
  await page.goto('/');await assignmentFlow(page,invoke);expect(errors).toEqual([]);
 }finally{child.kill();}
});
