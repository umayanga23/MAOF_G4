import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { ArrowLeft, Clock, GitFork, Play, ShieldCheck, X } from 'lucide-react';
import { useWorkspace } from '../app/workspace';
import type { Agent, Settings, Task, Workflow } from '../types';
import { Canvas } from './Canvas';
import { DEMO_PLAN, useDemoRun } from './demo';
import { initials, kTokens, latestRun, planFromWorkflow, runView, type Plan, type RunView, type Step, type StepStatus } from './model';

const DEMO_AGENTS = ['Research Agent', 'Data Agent', 'Writer Agent', 'Reviewer Agent', 'Mailer Agent'];
const STATUS_LABEL: Record<StepStatus, string> = { planned: 'Planned', queued: 'Queued', running: 'Running', waiting: 'Waiting', approval: 'Needs approval', done: 'Done', failed: 'Failed', cancelled: 'Cancelled' };
const STATUS_TONE: Record<StepStatus, string> = { planned: 'idle', queued: 'queued', running: 'running', waiting: 'idle', approval: 'paused', done: 'completed', failed: 'failed', cancelled: 'idle' };
const RUN_TONE: Record<RunView['status'], [string, string]> = {
  running: ['running', 'Running'], paused: ['paused', 'Paused'], stopping: ['paused', 'Stopping'], interrupted: ['failed', 'Interrupted'],
  approval: ['paused', 'Needs approval'], completed: ['completed', 'Completed'], failed: ['failed', 'Failed'], cancelled: ['idle', 'Cancelled'],
};

export function Badge({ tone, children }: { tone: string; children: ReactNode }) {
  return <span className={`wf-badge tone-${tone}`}><span className="wf-dot" />{children}</span>;
}

type Props = { demo?: boolean; workflow?: Workflow; prompt: string; onBack: () => void };

export function PlanView({ demo, workflow, prompt, onBack }: Props) {
  const { snapshot, send, busy } = useWorkspace();
  const [mode, setMode] = useState<'plan' | 'canvas'>('plan');
  const [draft, setDraft] = useState<Workflow | undefined>(workflow);
  const [selected, setSelected] = useState<string>();
  const [showRun, setShowRun] = useState(false);
  const [now, setNow] = useState(Date.now());
  const demoRun = useDemoRun();

  // Pick up engine changes (e.g. the full editor) unless there are unsaved edits here.
  const saved = JSON.stringify(workflow);
  const dirty = !!draft && JSON.stringify(draft) !== saved;
  useEffect(() => { if (!dirty) setDraft(workflow); }, [saved]); // eslint-disable-line react-hooks/exhaustive-deps

  const settings = snapshot?.workspace.settings;
  const agents = snapshot?.workspace.agents ?? [];
  const plan: Plan = useMemo(() => (demo || !draft || !settings ? DEMO_PLAN : planFromWorkflow(draft, agents, settings, snapshot?.workspace.documents ?? [])), [demo, draft, settings, agents, snapshot?.workspace.documents]);

  const run = workflow && snapshot ? latestRun(snapshot.workspace.runs, workflow) : undefined;
  const engineRun = run ? runView(run, now, c => void send(c)) : undefined;
  const view = demo ? demoRun.view : engineRun && (engineRun.active || showRun) ? engineRun : undefined;
  useEffect(() => {
    if (!view?.active) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [view?.active]);

  const steps = plan.stages.flatMap(s => s.steps);
  const current = steps.find(s => s.id === selected) ?? steps.find(s => s.instructions) ?? steps[0];
  const [tone, label] = view ? RUN_TONE[view.status] : ['idle', 'Draft'];

  const start = async () => {
    if (demo) { demoRun.start(); return; }
    if (!workflow) return;
    if (await send({ type: 'launch', workflow_id: workflow.id, prompt: prompt.trim() || `Run the “${workflow.name}” workflow.` })) setShowRun(true);
  };
  const stop = () => (demo ? demoRun.stop() : run && void send({ type: 'control', run_id: run.id, action: 'stop' }));
  const save = () => { if (draft) void send({ type: 'save_workflow', workflow: draft }); };
  const updateTask = (id: string, change: Partial<Task>) => setDraft(d => d && { ...d, tasks: d.tasks.map(t => (t.id === id ? { ...t, ...change } : t)) });

  return (
    <div className="wf-page">
      <div className="wf-header">
        <button type="button" className="mx-icon-btn" onClick={onBack} aria-label="Back to workflows" title="Back to workflows"><ArrowLeft size={20} strokeWidth={1.5} /></button>
        <div className="wf-title">
          <div className="wf-title-row"><h1 className="wf-h1">{plan.name}</h1><Badge tone={tone}>{label}</Badge>{plan.demo && <span className="wf-demo-tag">Demo data</span>}</div>
          <p className="wf-small">{plan.subtitle}</p>
        </div>
        <div className="wf-segmented" role="tablist" aria-label="View">
          <button type="button" role="tab" aria-selected={mode === 'plan'} onClick={() => setMode('plan')}>Plan</button>
          <button type="button" role="tab" aria-selected={mode === 'canvas'} onClick={() => setMode('canvas')}>Canvas</button>
        </div>
        <button type="button" className="wf-btn wf-btn-outline" onClick={save} disabled={demo || !dirty || busy}>Save</button>
        {!view && <button type="button" className="wf-btn wf-btn-primary" onClick={() => void start()} disabled={busy || plan.valid !== true || dirty} title={dirty ? 'Save your changes before running' : undefined}>Run</button>}
      </div>

      {plan.demo && <p className="wf-demo-note">Sample plan from the design. Schedules, decisions, e-mail and HTTP steps are not in the engine yet, so its run is simulated here and calls no agents.</p>}

      {view ? (
        <>
          <section className="wf-card wf-progress">
            <div className="wf-row">
              <strong>{view.title}</strong>
              <span className="wf-small">{view.detail}</span>
              {view.active
                ? <button type="button" className="wf-btn wf-btn-danger wf-push" onClick={stop}>Stop</button>
                : <span className="wf-push wf-row-tight">
                    {!demo && <button type="button" className="wf-btn wf-btn-ghost" onClick={() => setShowRun(false)}>Back to plan</button>}
                    <button type="button" className="wf-btn wf-btn-outline" onClick={() => (demo ? demoRun.start() : void start())}>Run again</button>
                  </span>}
            </div>
            <div className="wf-bar"><div className={`wf-bar-fill tone-${view.status === 'completed' ? 'completed' : view.status === 'failed' ? 'failed' : 'running'}`} style={{ width: `${Math.round(view.progress * 100)}%` }} /></div>
          </section>
          {view.approvals.map(a => (
            <section key={a.key} className="wf-approval" role="alert">
              <ShieldCheck size={20} strokeWidth={1.5} className="wf-approval-icon" />
              <div><strong>{a.title}</strong><p className="wf-small">{a.detail}</p></div>
              <button type="button" className="wf-btn wf-btn-danger wf-push" onClick={a.reject} disabled={busy}>Reject</button>
              <button type="button" className="wf-btn wf-btn-primary" onClick={a.approve} disabled={busy}>{a.approveLabel}</button>
            </section>
          ))}
        </>
      ) : (
        <Preflight plan={plan} lastRun={engineRun} onShowRun={() => setShowRun(true)} />
      )}

      <div className="wf-body">
        <div className="wf-main">
          {mode === 'plan'
            ? <Timeline plan={plan} run={view} selected={view ? undefined : current?.id} onSelect={setSelected} />
            : <Canvas plan={plan} run={view} selected={view ? undefined : current?.id} onSelect={setSelected} />}
        </div>
        <aside className="wf-card wf-inspector">
          {view ? (
            <>
              <h2 className="wf-inspector-head">Run log</h2>
              <div className="wf-log">{view.log.length ? view.log.map((line, i) => <p key={i}>{line}</p>) : <p>Waiting for the first event…</p>}</div>
            </>
          ) : current ? (
            <>
              <h2 className="wf-inspector-head">Step details</h2>
              <Inspector key={current.id} step={current} plan={plan} workflow={draft} agents={agents} settings={settings} dirty={dirty}
                onChange={change => updateTask(current.id, change)} onDiscard={() => setDraft(workflow)} />
            </>
          ) : <p className="wf-small wf-pad">This workflow has no steps yet.</p>}
        </aside>
      </div>
    </div>
  );
}

function Preflight({ plan, lastRun, onShowRun }: { plan: Plan; lastRun?: RunView; onShowRun: () => void }) {
  const groups = plan.stages.filter(s => s.steps.length > 1).length;
  const count = plan.stages.reduce((n, s) => n + s.steps.length, 0);
  return (
    <div className="wf-pills">
      <span className={`wf-pill ${plan.valid === true ? 'tone-running' : 'tone-failed'}`}>{plan.valid === true ? '✓ Valid' : `✕ ${plan.valid}`}</span>
      <span className="wf-pill">{count} steps · {groups} parallel group{groups === 1 ? '' : 's'}</span>
      <span className="wf-pill">~{kTokens(plan.tokens)} tokens ({Math.round((plan.tokens / plan.budget) * 100)}% of {kTokens(plan.budget)} budget)</span>
      {plan.duration && <span className="wf-pill"><Clock size={12} strokeWidth={2} />{plan.duration}</span>}
      {plan.approvalGates > 0 && <span className="wf-pill tone-paused">{plan.approvalGates} approval gate{plan.approvalGates === 1 ? '' : 's'}</span>}
      {lastRun && <button type="button" className="wf-pill wf-pill-link" onClick={onShowRun}>Last run: {RUN_TONE[lastRun.status][1]} · view</button>}
    </div>
  );
}

function railTone(step: Step | undefined, status: StepStatus | undefined, group = false) {
  if (status === 'done') return 'filled-completed';
  if (status === 'approval') return 'paused';
  if (status === 'running') return 'running';
  if (status === 'failed') return 'failed';
  if (group) return 'queued';
  return step?.kind === 'decision' ? 'paused' : 'brand';
}

function Timeline({ plan, run, selected, onSelect }: { plan: Plan; run?: RunView; selected?: string; onSelect: (id: string) => void }) {
  const rows: ReactNode[] = [];
  const statusOf = (id: string) => run?.steps[id];
  const triggerDone = !!run;
  rows.push(
    <Row key="trigger" glyph={<Play size={11} strokeWidth={2.5} />} tone={triggerDone ? 'filled-completed' : 'brand'} last={!plan.stages.length}>
      <div className="wf-step wf-trigger">
        <div className="wf-icon tone-brand"><Clock size={16} strokeWidth={1.75} /></div>
        <div className="wf-step-name"><strong>{plan.trigger.title}</strong><span className="wf-small">{plan.trigger.subtitle}</span></div>
        {run && <Badge tone="completed">Done</Badge>}
      </div>
    </Row>,
  );
  plan.stages.forEach((stage, i) => {
    const last = i === plan.stages.length - 1;
    if (stage.steps.length > 1) {
      const statuses = stage.steps.map(s => statusOf(s.id));
      const groupStatus = statuses.every(s => s === 'done') ? 'done' : statuses.find(s => s && s !== 'planned');
      rows.push(
        <Row key={stage.id} glyph="=" tone={railTone(undefined, groupStatus, true)} last={last}>
          <div className="wf-group">
            <div className="wf-group-head"><strong>{stage.title}</strong><span className="wf-small">Runs at the same time ({stage.steps.length})</span></div>
            {stage.steps.map(step => <StepCard key={step.id} step={step} status={statusOf(step.id)} selected={selected === step.id} onSelect={onSelect} />)}
          </div>
        </Row>,
      );
    } else {
      const step = stage.steps[0]!;
      rows.push(
        <Row key={stage.id} glyph={step.kind === 'decision' ? '?' : step.owner[0]?.toUpperCase()} tone={railTone(step, statusOf(step.id))} last={last}>
          <StepCard step={step} status={statusOf(step.id)} selected={selected === step.id} onSelect={onSelect} />
        </Row>,
      );
    }
  });
  return <div className="wf-timeline">{rows}</div>;
}

function Row({ glyph, tone, last, children }: { glyph: ReactNode; tone: string; last: boolean; children: ReactNode }) {
  return (
    <div className="wf-tl-row">
      <div className="wf-rail"><div className={`wf-node rail-${tone}`}>{glyph}</div>{!last && <div className="wf-rail-line" />}</div>
      <div className="wf-tl-body">{children}</div>
    </div>
  );
}

function StepCard({ step, status, selected, onSelect }: { step: Step; status?: StepStatus; selected: boolean; onSelect: (id: string) => void }) {
  const inRun = status && status !== 'planned';
  return (
    <div className={`wf-step${selected ? ' selected' : ''}${status === 'approval' ? ' attention' : ''}`} role="button" tabIndex={0} aria-pressed={selected}
      data-step={step.id} onClick={() => onSelect(step.id)} onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelect(step.id); } }}>
      <div className="wf-step-head">
        {step.kind === 'decision' ? <div className="wf-icon tone-paused"><GitFork size={16} strokeWidth={1.75} /></div> : <div className="wf-avatar">{initials(step.owner)}</div>}
        <div className="wf-step-name"><strong>{step.title}</strong><span className="wf-small">{step.owner}</span></div>
        {inRun ? <Badge tone={STATUS_TONE[status]}>{STATUS_LABEL[status]}</Badge> : step.estimate && <span className="wf-small">{step.estimate}</span>}
      </div>
      {step.instructions && !inRun && <p className="wf-instructions">{step.instructions}</p>}
      {step.chips.length > 0 && <div className="wf-chips">{step.chips.map(c => <span key={c.label} className={`wf-chip${c.tone ? ` tone-${c.tone === 'success' ? 'running' : 'paused'}` : ''}`}>{c.label}</span>)}</div>}
    </div>
  );
}

type InspectorProps = { step: Step; plan: Plan; workflow?: Workflow; agents: Agent[]; settings?: Settings; dirty: boolean; onChange: (c: Partial<Task>) => void; onDiscard: () => void };

function Inspector({ step, plan, workflow, agents, settings, dirty, onChange, onDiscard }: InspectorProps) {
  const task = workflow?.tasks.find(t => t.id === step.id);
  const [askFirst, setAskFirst] = useState(step.chips.some(c => c.label === 'Asks you first'));
  const nameOf = (id: string) => workflow?.tasks.find(t => t.id === id)?.name ?? id;
  const candidates = workflow?.tasks.filter(t => t.id !== step.id && !task?.dependencies.includes(t.id)) ?? [];
  const demoInputs = plan.stages.filter(s => s.steps.some(st => step.dependencies.includes(st.id))).map(s => s.title ?? s.steps[0]!.title);

  if (!task) {
    // Demo plan: every field is interactive but nothing is stored.
    return (
      <div className="wf-fields">
        <label className="wf-field">Name<input defaultValue={step.title} /></label>
        <label className="wf-field">Agent
          <select defaultValue={step.kind === 'decision' ? '' : step.owner} disabled={step.kind === 'decision'}>
            {step.kind === 'decision' && <option value="">Rule (no agent)</option>}
            {DEMO_AGENTS.map(a => <option key={a}>{a}</option>)}
          </select>
        </label>
        <label className="wf-field">What should it do?<textarea rows={3} defaultValue={step.instructions ?? ''} placeholder="Describe this step in plain words." /></label>
        <div className="wf-field">Gets input from
          <div className="wf-chips">{(demoInputs.length ? demoInputs : ['Trigger']).map(n => <span key={n} className="wf-chip">{n}</span>)}<button type="button" className="wf-chip wf-chip-btn">+ Add</button></div>
        </div>
        <Toggle label="Ask me before this step" hint="Pauses the run until you approve." checked={askFirst} onChange={setAskFirst} />
        <label className="wf-field">If it fails
          <select defaultValue="retry2"><option value="retry2">Retry 2 times, then stop</option><option value="retry1">Retry once, then stop</option><option value="stop">Stop the run</option><option value="skip">Skip this step</option></select>
        </label>
      </div>
    );
  }

  const capability = task.assignment === 'capabilities';
  return (
    <div className="wf-fields">
      <label className="wf-field">Name<input value={task.name} maxLength={100} onChange={e => onChange({ name: e.target.value })} /></label>
      <label className="wf-field">Agent
        <select value={capability ? '' : task.agent_id} disabled={capability} onChange={e => onChange({ agent_id: e.target.value })}>
          {capability && <option value="">Chosen by capabilities: {task.required_capabilities.join(', ')}</option>}
          {agents.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
      </label>
      <label className="wf-field">What should it do?
        <textarea rows={3} disabled placeholder="Per-step instructions are not stored by the engine yet. Each agent follows its own instructions and the run prompt." />
      </label>
      <div className="wf-field">Gets input from
        <div className="wf-chips">
          {task.dependencies.length === 0 && <span className="wf-chip">Run prompt</span>}
          {task.dependencies.map(d => (
            <span key={d} className="wf-chip">{nameOf(d)}
              <button type="button" className="wf-chip-x" aria-label={`Remove ${nameOf(d)}`} onClick={() => onChange({ dependencies: task.dependencies.filter(x => x !== d) })}><X size={12} strokeWidth={2} /></button>
            </span>
          ))}
          {candidates.length > 0 && (
            <select className="wf-chip wf-chip-btn" aria-label="Add an input" value="" onChange={e => e.target.value && onChange({ dependencies: [...task.dependencies, e.target.value] })}>
              <option value="">+ Add</option>
              {candidates.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}
            </select>
          )}
        </div>
      </div>
      <Toggle label="Ask me before this step" hint="Not per step yet: an agent's report permission decides this." checked={step.chips.some(c => c.label === 'Asks you first')} disabled onChange={() => {}} />
      <label className="wf-field">If it fails
        <select disabled value="ws"><option value="ws">{retryLabel(settings?.max_retries ?? 0)} (workspace setting)</option></select>
      </label>
      <div className="wf-inspector-actions">
        {dirty && <button type="button" className="wf-btn wf-btn-ghost" onClick={onDiscard}>Discard changes</button>}
        <button type="button" className="wf-btn wf-btn-outline wf-push" data-edit-workflow={dirty ? undefined : workflow!.id} disabled={dirty} title={dirty ? 'Save or discard your changes first' : 'Every task field, including capabilities, timing and documents'}>Open full editor</button>
      </div>
    </div>
  );
}

function retryLabel(n: number) {
  return n === 0 ? 'Stop the run' : `Retry ${n === 1 ? 'once' : `${n} times`}, then stop`;
}

function Toggle({ label, hint, checked, disabled, onChange }: { label: string; hint: string; checked: boolean; disabled?: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="wf-toggle-row">
      <div><strong>{label}</strong><p className="wf-small">{hint}</p></div>
      <button type="button" role="switch" aria-checked={checked} aria-label={label} className="wf-toggle" disabled={disabled} onClick={() => onChange(!checked)}><span /></button>
    </div>
  );
}
