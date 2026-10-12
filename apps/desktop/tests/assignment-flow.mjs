import {expect} from '@playwright/test';
import {readFileSync} from 'node:fs';

// The same user workflow runs against the Rust bridge and the actual Tauri app.
export async function assignmentFlow(page,invoke,screenshot){
 await expect(page.getByRole('heading',{name:'Give your agents a place to work.'})).toBeVisible();
 await page.locator('nav [data-nav="documents"]').click();
 await page.getByLabel('Text or Markdown file').setInputFiles({name:'assignment-note.md',mimeType:'text/markdown',buffer:Buffer.from('CAPABILITY_INPUT: compare task scheduling strategies.')});
 await page.getByRole('button',{name:'Add document',exact:true}).click();
 await expect(page.getByRole('cell',{name:'assignment-note.md',exact:false})).toBeVisible();
 const document=(await invoke('snapshot')).workspace.documents.at(-1);
 await page.locator('nav [data-nav="workflows"]').click();
 await page.getByRole('button',{name:'New workflow',exact:true}).click();
 await page.getByLabel('Workflow name',{exact:true}).fill('Automatic research');
 await expect(page.locator('[name="capabilities-0"]')).toBeDisabled();
 await page.locator('[name="assignment-0"]').selectOption('capabilities');
 await expect(page.locator('[name="agent-0"]')).toBeDisabled();
 await expect(page.locator('[name="capabilities-0"]')).toBeEnabled();
 await page.locator('[name="capabilities-0"]').fill('research, analysis');
 await page.locator('[name="steps-0"]').fill('1');
 await page.locator('[name="report-0"]').check();
 await page.locator('[name="document-0"]').selectOption(document.id);
 await page.getByRole('button',{name:'Save workflow',exact:true}).click();
 await expect(page.getByRole('dialog')).not.toBeVisible();
 const template=(await invoke('snapshot')).workspace.workflows.at(-1);
 expect(template.tasks[0].agent_id).toBe('');
 expect(template.tasks[0].required_capabilities).toEqual(['research','analysis']);
 await page.locator('nav [data-nav="overview"]').click();
 await page.getByLabel('Workflow template').selectOption(template.id);
 await page.getByRole('button',{name:'Start workflow',exact:true}).click();
 await expect(page.getByRole('status')).toContainText('No agent matches all capabilities');
 expect((await invoke('snapshot')).workspace.runs).toHaveLength(0);
 for(const [id,priority,read] of [['researcher',1,'deny'],['analyst',3,'ask'],['writer',6,'ask']]){
  await page.locator('nav [data-nav="agents"]').click();
  await page.locator(`[data-edit-agent="${id}"]`).click();
  await page.getByLabel('Capabilities (comma separated)',{exact:true}).fill('research, analysis');
  await page.locator('[name="priority"]').fill(String(priority));
  await page.getByLabel('Permission to read selected documents').selectOption(read);
  await page.getByRole('button',{name:'Save agent',exact:true}).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
 }
 await page.locator('nav [data-nav="overview"]').click();
 await page.getByRole('button',{name:'Start workflow',exact:true}).click();
 await expect(page.getByRole('heading',{name:'Agent assignment',exact:true})).toBeVisible();
 await expect.poll(async()=>(await invoke('snapshot')).workspace.runs.at(-1).tasks[0].status).toBe('approval');
 let run=(await invoke('snapshot')).workspace.runs.at(-1);
 expect(run.tasks[0].agent.id).toBe('analyst');
 expect(run.tasks[0].assignment_decision.eligible_agent_ids).toEqual(['analyst','writer']);
 expect(run.tasks[0].reserved_tokens).toBe(0);expect(run.api_calls).toBe(0);expect(run.tool_calls).toBe(0);
 const pinned=structuredClone(run.tasks[0].agent);
 // Editor changes affect the next run; current approvals keep their saved policy.
 await page.locator('nav [data-nav="agents"]').click();
 await page.locator('[data-edit-agent="analyst"]').click();
 await page.getByLabel('Capabilities (comma separated)',{exact:true}).fill('');
 await page.locator('[name="priority"]').fill('9');
 await page.getByLabel('Permission to read selected documents').selectOption('deny');
 await page.getByLabel('Permission to save a local report').selectOption('deny');
 await page.getByRole('button',{name:'Save agent',exact:true}).click();
 await expect(page.getByRole('dialog')).not.toBeVisible();
 await page.locator('nav [data-nav="approvals"]').click();
 await expect(page.getByRole('button',{name:'Allow this document',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Allow this document',exact:true}).click();
 await expect(page.getByRole('button',{name:'Allow this report',exact:true})).toBeVisible();
 run=(await invoke('snapshot')).workspace.runs.at(-1);
 expect(run.artifacts).toHaveLength(0);expect(run.tasks[0].reserved_tokens).toBe(0);
 await page.getByRole('button',{name:'Allow this report',exact:true}).click();
 await expect.poll(async()=>(await invoke('snapshot')).workspace.runs.at(-1).status).toBe('completed');
 run=(await invoke('snapshot')).workspace.runs.at(-1);
 expect(run.tasks[0].agent).toEqual(pinned);expect(run.api_calls).toBe(1);expect(run.tool_calls).toBe(1);expect(run.artifacts).toHaveLength(1);
 expect(run.memory.some(e=>e.kind==='document'&&e.text.includes('CAPABILITY_INPUT'))).toBe(true);
 await page.locator('nav [data-nav="runs"]').click();
 await page.getByRole('button',{name:'Open',exact:false}).click();
 await expect(page.locator('.assignment-record')).toContainText('Analyst (version 2)');
 await page.getByRole('button',{name:'Export JSON',exact:true}).click();
 await expect(page.getByRole('status')).toContainText('Saved:');
 const exported=JSON.parse(readFileSync((await page.getByRole('status').textContent()).replace('Saved: ',''),'utf8'));
 expect(exported.tasks[0].assignment_decision).toEqual(run.tasks[0].assignment_decision);
 if(screenshot){await expect(page.locator('#toast.visible')).toHaveCount(0,{timeout:10000});await page.screenshot({path:screenshot,fullPage:true});}
 await page.locator('nav [data-nav="overview"]').click();
 await page.getByRole('button',{name:'Start workflow',exact:true}).click();
 await expect.poll(async()=>(await invoke('snapshot')).workspace.runs.at(-1).tasks[0].status).toBe('approval');
 const next=(await invoke('snapshot')).workspace.runs.at(-1);
 expect(next.tasks[0].agent.id).toBe('writer');
 await page.locator('nav [data-nav="approvals"]').click();
 await page.getByRole('button',{name:'Reject document read',exact:true}).click();
 await expect.poll(async()=>(await invoke('snapshot')).workspace.runs.at(-1).status).toBe('failed');
 const rejected=(await invoke('snapshot')).workspace.runs.at(-1);
 expect(rejected.api_calls).toBe(0);expect(rejected.tool_calls).toBe(0);
 return {chosen_agent:run.tasks[0].agent.id,pinned_version:pinned.version,eligible_ids:run.tasks[0].assignment_decision.eligible_agent_ids,
  no_match_atomic:true,deny_filtered:true,document_and_report_approved:true,edits_do_not_reroute:true,next_run_uses_edits:true,rejection_prevents_calls:true,exported_decision:true};
}
