import type { Agent, Run, Settings, Task, TaskRun, Workflow } from '../types';

export type StepStatus = 'planned' | 'queued' | 'running' | 'waiting' | 'approval' | 'done' | 'failed' | 'cancelled';
export type Tone = 'success' | 'warning';

export interface Chip { label: string; tone?: Tone }
export interface Step {
  id: string;
  kind: 'agent' | 'decision';
  title: string;
  /** Agent name, or "Decision". */
  owner: string;
  estimate?: string;
  instructions?: string;
  chips: Chip[];
  dependencies: string[];
}
export interface Stage { id: string; title?: string; steps: Step[] }
export interface Plan {
  id: string;
  name: string;
  /** True for the built-in sample: nothing in it is executed by the engine. */
  demo: boolean;
  subtitle: string;
  trigger: { title: string; subtitle: string };
  stages: Stage[];
  /** Retry edge drawn on the canvas, e.g. a decision sending work back. */
  loop?: { from: string; to: string; label: string };
  valid: true | string;
  tokens: number;
  budget: number;
  duration?: string;
  approvalGates: number;
}

export interface Approval { key: string; title: string; detail: string; approveLabel: string; approve: () => void; reject: () => void }
export interface RunView {
  status: 'running' | 'paused' | 'stopping' | 'interrupted' | 'approval' | 'completed' | 'failed' | 'cancelled';
  title: string;
  detail: string;
  progress: number;
  steps: Record<string, StepStatus>;
  approvals: Approval[];
  log: string[];
  active: boolean;
}

export const initials = (name: string) => name.split(/\s+/).filter(Boolean).slice(0, 2).map(w => w[0]!.toUpperCase()).join('') || '?';
export const kTokens = (n: number) => (n >= 1000 ? `${Math.round(n / 100) / 10}k` : String(n)).replace('.0k', 'k');
export const clock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
};

/** "~2 s", "~3 min" for estimates. */
export const roughly = (ms: number) => (ms < 60000 ? `~${Math.max(1, Math.round(ms / 1000))} s` : `~${Math.round(ms / 60000)} min`);

/** Dependency depth of every task; tasks at the same depth can run at the same time. Returns null on a cycle. */
export function levels(tasks: Task[]): Map<string, number> | null {
  const byId = new Map(tasks.map(t => [t.id, t]));
  const depth = new Map<string, number>();
  const visiting = new Set<string>();
  const visit = (id: string): number | null => {
    if (depth.has(id)) return depth.get(id)!;
    if (visiting.has(id)) return null;
    visiting.add(id);
    let d = 0;
    for (const dep of byId.get(id)?.dependencies ?? []) {
      if (!byId.has(dep)) continue;
      const parent = visit(dep);
      if (parent === null) return null;
      d = Math.max(d, parent + 1);
    }
    visiting.delete(id);
    depth.set(id, d);
    return d;
  };
  for (const t of tasks) if (visit(t.id) === null) return null;
  return depth;
}

function agentFor(task: Task, agents: Agent[]) {
  return task.assignment === 'capabilities' ? undefined : agents.find(a => a.id === task.agent_id);
}

export function planFromWorkflow(workflow: Workflow, agents: Agent[], settings: Settings, documents: { id: string; name: string }[]): Plan {
  const depth = levels(workflow.tasks);
  const stagesByDepth = new Map<number, Task[]>();
  workflow.tasks.forEach((t, i) => {
    const d = depth?.get(t.id) ?? i;
    stagesByDepth.set(d, [...(stagesByDepth.get(d) ?? []), t]);
  });
  let parallel = 0;
  const stages: Stage[] = [...stagesByDepth.entries()].sort(([a], [b]) => a - b).map(([d, tasks]) => {
    if (tasks.length > 1) parallel += 1;
    return {
      id: `stage-${d}`,
      title: tasks.length > 1 ? `Parallel group ${parallel}` : undefined,
      steps: tasks.map(t => {
        const agent = agentFor(t, agents);
        const chips: Chip[] = [];
        if (t.document_id) chips.push({ label: `Reads ${documents.find(doc => doc.id === t.document_id)?.name ?? 'a document'}` });
        if (t.publish_report) chips.push({ label: 'Saves report' });
        if (t.publish_report && agent?.report_permission === 'ask') chips.push({ label: 'Asks you first', tone: 'warning' });
        if (t.assignment === 'capabilities') chips.push({ label: `Needs: ${t.required_capabilities.join(', ')}` });
        return {
          id: t.id,
          kind: 'agent' as const,
          title: t.name,
          owner: agent?.name ?? (t.assignment === 'capabilities' ? 'Assigned automatically' : t.agent_id),
          estimate: `${kTokens(t.steps * settings.step_tokens)} tokens · ${t.steps} step${t.steps === 1 ? '' : 's'}`,
          chips,
          dependencies: t.dependencies,
        };
      }),
    };
  });
  const tokens = workflow.tasks.reduce((n, t) => n + t.steps * settings.step_tokens, 0);
  const longest = stages.reduce((n, s) => n + Math.max(...s.steps.map(st => workflow.tasks.find(t => t.id === st.id)!.steps)), 0);
  return {
    id: workflow.id,
    name: workflow.name,
    demo: false,
    subtitle: `${workflow.tasks.length} tasks · ${new Set(workflow.tasks.map(t => t.agent_id)).size} agents · ID ${workflow.id}`,
    trigger: { title: 'Start manually', subtitle: 'Runs when you press Run' },
    stages,
    valid: depth ? true : 'Dependency cycle',
    tokens,
    budget: settings.token_budget,
    duration: settings.mode === 'demo' ? `${roughly(longest * settings.demo_delay_ms)} in Demo mode` : undefined,
    approvalGates: workflow.tasks.filter(t => t.publish_report && agentFor(t, agents)?.report_permission === 'ask').length,
  };
}

const stepStatus: Record<string, StepStatus> = { ready: 'queued', running: 'running', waiting: 'waiting', approval: 'approval', completed: 'done', failed: 'failed', cancelled: 'cancelled' };

export function runView(run: Run, now: number, send: (c: import('../app/workspace').Command) => void): RunView {
  const done = run.tasks.filter(t => t.status === 'completed').length;
  const tokens = run.tasks.reduce((n, t) => n + t.tokens, 0);
  const pending = run.tasks.filter(t => t.status === 'approval');
  const approvals = pending.map((t: TaskRun): Approval => {
    const doc = t.document && !t.document.done ? t.document : null;
    if (doc) {
      const decide = (allow: boolean) => send({ type: 'approve_document', run_id: run.id, task_id: t.definition.id, request_id: doc.request.id, allow });
      return { key: `${t.definition.id}:doc`, title: `${t.agent.name} wants to read ${doc.request.document.name}`, detail: `${t.definition.name} · up to ${doc.request.document.bytes} bytes`, approveLabel: 'Allow this read', approve: () => decide(true), reject: () => decide(false) };
    }
    const decide = (allow: boolean) => send({ type: 'approve', run_id: run.id, task_id: t.definition.id, allow });
    const words = t.output.trim() ? t.output.trim().split(/\s+/).length : 0;
    return { key: `${t.definition.id}:report`, title: `${t.agent.name} wants to save a report`, detail: `${t.definition.name} · ${words}-word report, saved in this run only`, approveLabel: 'Approve and save', approve: () => decide(true), reject: () => decide(false) };
  });
  const status: RunView['status'] = pending.length && !['completed', 'failed', 'cancelled'].includes(run.status) ? 'approval' : (run.status as RunView['status']);
  const titles: Record<RunView['status'], string> = {
    running: 'Running', paused: 'Paused', stopping: 'Stopping…', interrupted: 'Interrupted — recover it from Tasks',
    approval: 'Waiting for your approval', completed: 'Run completed', failed: 'Run failed', cancelled: 'Run cancelled',
  };
  const elapsed = (run.finished_at ?? now) - run.started_at;
  return {
    status,
    title: titles[status],
    detail: `${done} of ${run.tasks.length} steps · ${clock(elapsed)} · ${kTokens(tokens)} of ${kTokens(run.settings.token_budget)} tokens`,
    progress: run.tasks.length ? done / run.tasks.length : 0,
    steps: Object.fromEntries(run.tasks.map(t => [t.definition.id, stepStatus[t.status] ?? 'planned'])),
    approvals,
    log: run.events.slice(-12).map(e => `${clock(e.at_ms - run.started_at)}  ${e.message}`),
    active: !['completed', 'failed', 'cancelled'].includes(run.status),
  };
}

/** Runs are named after their workflow; prefer an unfinished one, else the latest. */
export function latestRun(runs: Run[], workflow: Workflow) {
  const mine = runs.filter(r => r.name === workflow.name);
  return mine.find(r => !['completed', 'failed', 'cancelled'].includes(r.status)) ?? mine.at(-1);
}
