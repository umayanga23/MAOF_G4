import { useCallback, useEffect, useRef, useState } from 'react';
import { clock, type Plan, type RunView, type StepStatus } from './model';

/**
 * Sample workflow from the MAOF_UI Figma file. The engine has no schedules, decision steps,
 * e-mail or HTTP tools, so this plan and its run are simulated in the browser only.
 */
export const DEMO_PLAN: Plan = {
  id: 'demo-weekly-market-report',
  name: 'Weekly market report',
  demo: true,
  subtitle: 'Version 3 · edited 2 min ago',
  trigger: { title: 'Every Monday at 09:00', subtitle: 'Schedule' },
  stages: [
    {
      id: 'gather', title: 'Gather inputs', steps: [
        { id: 'news', kind: 'agent', title: 'Collect Q3 market news', owner: 'Research Agent', estimate: '4k tokens · ~2 min', chips: [], dependencies: [] },
        { id: 'sales', kind: 'agent', title: 'Pull sales figures', owner: 'Data Agent', estimate: '2k tokens · ~1 min', chips: [], dependencies: [] },
      ],
    },
    { id: 'draft-stage', steps: [{ id: 'draft', kind: 'agent', title: 'Draft the report', owner: 'Writer Agent', estimate: '6k tokens · ~2 min', instructions: 'Write a 400-word report from the news and the sales figures. Keep a neutral tone.', chips: [{ label: 'File write' }], dependencies: ['news', 'sales'] }] },
    { id: 'decision-stage', steps: [{ id: 'decision', kind: 'decision', title: 'Quality score is 80 or higher?', owner: 'Decision', chips: [{ label: 'Yes: Continue', tone: 'success' }, { label: 'No: Back to draft, up to 2 times', tone: 'warning' }], dependencies: ['draft'] }] },
    { id: 'review-stage', steps: [{ id: 'review', kind: 'agent', title: 'Check facts and tone', owner: 'Reviewer Agent', estimate: '3k tokens · ~1 min', chips: [], dependencies: ['decision'] }] },
    { id: 'email-stage', steps: [{ id: 'email', kind: 'agent', title: 'Email the report to the team', owner: 'Mailer Agent', estimate: '1k tokens · ~10 s', chips: [{ label: 'HTTP request' }, { label: 'Asks you first', tone: 'warning' }], dependencies: ['review'] }] },
  ],
  loop: { from: 'decision', to: 'draft', label: 'No: retry up to 2×' },
  valid: true,
  tokens: 16000,
  budget: 200000,
  duration: '~6 min',
  approvalGates: 1,
};

const ORDER = [['news', 'sales'], ['draft'], ['decision'], ['review'], ['email']];
const LOG = ['Trigger fired: Monday 09:00', 'Gather inputs finished', 'Draft ready (620 words)', 'Quality score 86 (80+): continue', 'Review passed'];
const TICK_MS = 1200;

type Phase = { stage: number; state: 'running' | 'approval' | 'completed' | 'failed' | 'cancelled'; started: number; finished?: number };

/** Steps through DEMO_PLAN on a timer and stops at the e-mail approval gate. */
export function useDemoRun() {
  const [phase, setPhase] = useState<Phase | null>(null);
  const [now, setNow] = useState(Date.now());
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (phase?.state !== 'running') return;
    timer.current = window.setTimeout(() => {
      setPhase(p => {
        if (!p || p.state !== 'running') return p;
        const next = p.stage + 1;
        return next === ORDER.length - 1 ? { ...p, stage: next, state: 'approval' } : { ...p, stage: next };
      });
    }, TICK_MS);
    return () => window.clearTimeout(timer.current);
  }, [phase]);

  useEffect(() => {
    if (!phase || ['completed', 'failed', 'cancelled'].includes(phase.state)) return;
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(id);
  }, [phase]);

  const start = useCallback(() => setPhase({ stage: 0, state: 'running', started: Date.now() }), []);
  const finish = (state: Phase['state']) => setPhase(p => (p ? { ...p, state, finished: Date.now(), stage: state === 'completed' ? ORDER.length : p.stage } : p));
  const reset = useCallback(() => setPhase(null), []);

  if (!phase) return { view: undefined, start, reset, stop: () => {} };

  const steps: Record<string, StepStatus> = {};
  ORDER.forEach((ids, i) => ids.forEach(id => {
    steps[id] = i < phase.stage ? 'done'
      : i > phase.stage ? 'planned'
      : phase.state === 'approval' ? 'approval'
      : phase.state === 'running' ? 'running'
      : phase.state === 'completed' ? 'done' : 'cancelled';
  }));
  const doneSteps = Object.values(steps).filter(s => s === 'done').length + 1; // + trigger
  const elapsed = (phase.finished ?? now) - phase.started;
  const log = LOG.slice(0, Math.min(phase.stage + 1, LOG.length)).map((line, i) => `${clock(i * 6000)}  ${line}`);
  if (phase.state === 'approval') log.push(`${clock(36000)}  Waiting for your approval to send the email`);
  if (phase.state === 'completed') log.push(`${clock(41000)}  Email sent after your approval`);
  if (phase.state === 'failed') log.push(`${clock(36000)}  Email rejected; run stopped`);
  if (phase.state === 'cancelled') log.push(`${clock(elapsed)}  Run stopped by you`);
  const titles = { running: 'Running', approval: 'Waiting for your approval', completed: 'Run completed', failed: 'Run failed', cancelled: 'Run cancelled' } as const;

  const view: RunView = {
    status: phase.state,
    title: titles[phase.state],
    detail: `${doneSteps} of 7 steps · ${clock(elapsed)} · ${Math.min(16, 3 + phase.stage * 3)}k of 200k tokens`,
    progress: doneSteps / 7,
    steps,
    approvals: phase.state === 'approval' ? [{
      key: 'demo-email', title: 'Mailer Agent wants to send an email', detail: 'To team@company.com · 620-word report attached', approveLabel: 'Approve and send',
      approve: () => finish('completed'), reject: () => finish('failed'),
    }] : [],
    log,
    active: phase.state === 'running' || phase.state === 'approval',
  };
  return { view, start, reset, stop: () => finish('cancelled') };
}
