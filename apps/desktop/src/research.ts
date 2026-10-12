import type { ExperimentConfig, ExperimentReport, Run } from './types';

const escape = (v: unknown) => String(v ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const label = (s: string) => s.replaceAll('_', ' ').replace(/^./, c => c.toUpperCase());
const metric = (n: number | null) => n === null ? '—' : n.toFixed(1);
export const defaultExperiment: ExperimentConfig = {seed:42,trials:5,tasks:12,worker_slots:2,memory_mb:1024,arrival_spread_ms:100,max_step_ms:100,aging_quanta:2,workload:'mixed'};

export function readExperiment(form: HTMLFormElement): ExperimentConfig {
  const data = new FormData(form);
  const config = {...defaultExperiment, workload: data.get('workload') as ExperimentConfig['workload']};
  for (const key of ['seed','trials','tasks','worker_slots','memory_mb','arrival_spread_ms','max_step_ms','aging_quanta'] as const) config[key] = Number(data.get(key));
  return config;
}

export function researchView(config: ExperimentConfig, report: ExperimentReport | undefined, busy: boolean, active: boolean): string {
  const number = (key: keyof ExperimentConfig, title: string, min: number, max: number) => `<label>${title}<input type="number" name="${key}" value="${config[key]}" required min="${min}" max="${max}" step="1"></label>`;
  const maxWait = Math.max(1, ...report?.summaries.map(s=>s.mean_ready_wait_ms+s.mean_resource_wait_ms+s.mean_dependency_wait_ms)??[]);
  return `<div class="notice"><strong>Simulated workloads, real scheduling code.</strong> All four policies receive the same generated tasks in each trial. Times below are virtual milliseconds; no AI service is called.</div>
  <form id="experiment-form" class="panel settings-panel">
    <div class="panel-heading"><div><h2>Design an experiment</h2><p>Keep the seed to reproduce inputs and decisions.</p></div><span class="subtle-label">FIFO · ROUND ROBIN · STRICT PRIORITY · AGING</span></div>
    <div class="form-grid experiment-form-grid">
      <label>Workload<select name="workload">${['balanced','mixed','memory_pressure'].map(w=>`<option value="${w}" ${config.workload===w?'selected':''}>${label(w)}</option>`).join('')}</select></label>
      ${number('seed','Seed',0,4294967295)}${number('trials','Trials',1,20)}${number('tasks','Tasks per trial',3,24)}
      ${number('worker_slots','Workers',1,8)}${number('memory_mb','Memory capacity (MB)',64,8192)}
      ${number('arrival_spread_ms','Arrival window (virtual ms)',0,10000)}${number('max_step_ms','Maximum step duration (virtual ms)',1,1000)}
      ${number('aging_quanta','Dispatches per priority improvement',1,1000)}
    </div>
    <p class="hint">Balanced: equal work per task. Mixed: variable steps, durations and some dependencies. Memory pressure: competing memory requests, including rejection when a task is too large.</p>
    <div class="form-bottom"><span>Trials use different seeded inputs. Policies share each trial's inputs.</span><button type="submit" class="primary" data-compare ${busy?'disabled':''}>${busy?'Comparing…':'Run comparison'}</button></div>
  </form>
  ${report?`<div class="section-line"><div><h2>Comparison results</h2><p>Seed ${report.config.seed} · ${report.config.trials} trials · ${report.config.tasks} tasks/trial · ${label(report.config.workload)} · ${report.config.worker_slots} workers</p></div><button data-export-experiment ${busy?'disabled':''}>Export JSON + CSV</button></div>
  <div class="experiment-grid">${report.summaries.map(s=>`<section class="panel experiment"><h2>${label(s.policy)}</h2><span>Mean turnaround</span><strong class="big-number">${metric(s.mean_turnaround_ms)}<small> virtual ms</small></strong><div class="agent-detail"><span>P95 turnaround</span><strong>${metric(s.p95_turnaround_ms)}</strong></div><div class="agent-detail"><span>Mean first response</span><strong>${metric(s.mean_response_ms)}</strong></div><div class="agent-detail"><span>Completed / total</span><strong>${s.completed} / ${s.total}</strong></div><div class="agent-detail"><span>Rejected or failed</span><strong>${s.rejected}</strong></div></section>`).join('')}</div>
  <section class="panel"><div class="panel-heading"><div><h2>Where tasks waited</h2><p>Mean wait per task in virtual milliseconds. Each interval is counted once.</p></div></div>
    <div class="wait-legend"><span class="ready-key">Ready / workers</span><span class="resource-key">Memory / resources</span><span class="dependency-key">Dependencies</span></div>
    ${report.summaries.map(s=>`<div class="wait-row"><strong>${label(s.policy)}</strong><div class="wait-bar" role="img" aria-label="${label(s.policy)}: ready ${metric(s.mean_ready_wait_ms)}, resource ${metric(s.mean_resource_wait_ms)}, dependency ${metric(s.mean_dependency_wait_ms)} virtual milliseconds"><i class="ready-segment" style="width:${100*s.mean_ready_wait_ms/maxWait}%"></i><i class="resource-segment" style="width:${100*s.mean_resource_wait_ms/maxWait}%"></i><i class="dependency-segment" style="width:${100*s.mean_dependency_wait_ms/maxWait}%"></i></div><small>${metric(s.mean_ready_wait_ms)} / ${metric(s.mean_resource_wait_ms)} / ${metric(s.mean_dependency_wait_ms)}</small></div>`).join('')}
    <p class="hint">Turnaround and P95 use completed tasks. A dash means no observations. Compare completion counts before comparing averages. Waiting attribution follows the engine's first blocking reason; it does not prove what caused all delay.</p>
  </section>
  <section class="panel"><div class="panel-heading"><div><h2>Inspect dispatch decisions</h2><p>First trial shown; full per-task observations and traces are in the export.</p></div></div>${report.trials.filter(t=>t.trial===1).map(t=>`<details class="trace-details"><summary>${label(t.policy)} · ${t.trace.length} steps · finishes at ${t.makespan_ms} virtual ms</summary><div class="trace-scroll"><table><thead><tr><th>At (virtual ms)</th><th>Task</th><th>Step</th><th>Duration</th></tr></thead><tbody>${t.trace.map(s=>`<tr><td>${s.at_ms}</td><td>${escape(s.task_id)}</td><td>${s.step}</td><td>${s.duration_ms}</td></tr>`).join('')}</tbody></table></div></details>`).join('')}</section>`:
    '<section class="empty compact"><h2>Set up your first comparison.</h2><p>Change the workload, capacity or arrival window, then inspect all four policies.</p></section>'}
`;
}

export function resourcesView(run: Run): string {
  if (!run.tasks.some(t=>t.definition.resource_plan.length)) return '';
  return `<section class="panel resource-panel"><div class="panel-heading"><div><h2>Logical resources and recovery</h2><p>Demo ownership graph · version ${run.resource_version}</p></div><span class="subtle-label">${run.deadlock_recoveries.length} CYCLE(S) RECOVERED</span></div>
    <div class="ownership-list">${Object.entries(run.resource_owners).map(([r, owner])=>`<span><strong>${escape(r)}</strong> held by ${escape(owner)}</span>`).join('')||'<span>All logical resources are available.</span>'}</div>
    ${run.tasks.filter(t=>t.wait?.kind==='resource').map(t=>`<p>${escape(t.definition.id)} waits for <strong>${escape(t.wait?.resource)}</strong>, held by ${escape(t.wait?.owner)}.</p>`).join('')}
    ${run.deadlock_recoveries.map(r=>`<div class="recovery-record"><h3>Cycle confirmed · cancelled ${escape(r.victim)}</h3><p>${r.witness.edges.map(e=>`${escape(e.waiter)} → ${escape(e.resource)} → ${escape(e.owner)}`).join('<br>')}</p><small>Revalidated ownership version ${r.witness.resource_version}. Released ${escape(r.released.join(', '))}. Surviving tasks may continue; the cancelled task is not retried.</small></div>`).join('')}
  </section>`;
}
