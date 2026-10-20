import { useEffect, useMemo, useState } from 'react';
import { useWorkspace } from '../app/workspace';
import type { Workflow } from '../types';
import { DEMO_PLAN } from './demo';
import { levels } from './model';
import { PlanView } from './PlanView';
import './workflows.css';

const EXAMPLES = ['Weekly market report', 'Triage support tickets', 'Review a pull request'];
const DEMO_PROMPT = 'Every Monday, research market news, write a short report, have it reviewed, then email it to the team.';
// Template cards from the design, shown when no engine is connected. Each opens the sample plan.
const DEMO_TEMPLATES = [
  { name: 'Research → Write → Review', detail: '3 agents · sequential' },
  { name: 'Triage and route', detail: '4 agents · with decision' },
  { name: 'Data clean-up', detail: '2 agents · parallel' },
  { name: 'Code review loop', detail: '3 agents · repeats up to 3×' },
];

type Open = { kind: 'demo' } | { kind: 'workflow'; id: string } | null;

function describe(workflow: Workflow) {
  const agents = new Set(workflow.tasks.map(t => t.agent_id)).size;
  const depth = levels(workflow.tasks);
  const parallel = depth && new Set(depth.values()).size < workflow.tasks.length;
  return `${agents} agent${agents === 1 ? '' : 's'} · ${parallel ? 'parallel' : 'sequential'}`;
}

export function WorkflowsPage({ query, onCrumb }: { query: string; onCrumb: (label: string) => void }) {
  const { connected, snapshot, toast } = useWorkspace();
  const [open, setOpen] = useState<Open>(null);
  const [prompt, setPrompt] = useState('');
  const workflows = snapshot?.workspace.workflows ?? [];
  const workflow = open?.kind === 'workflow' ? workflows.find(w => w.id === open.id) : undefined;

  useEffect(() => {
    onCrumb(open?.kind === 'demo' ? DEMO_PLAN.name : workflow?.name ?? 'New workflow');
  }, [open, workflow, onCrumb]);

  const q = query.trim().toLowerCase();
  const cards = useMemo(() => (connected
    ? workflows.map(w => ({ key: w.id, name: w.name, detail: describe(w), open: () => setOpen({ kind: 'workflow', id: w.id }) }))
    : DEMO_TEMPLATES.map(t => ({ key: t.name, name: t.name, detail: `${t.detail} · demo`, open: () => setOpen({ kind: 'demo' }) }))
  ).filter(c => !q || c.name.toLowerCase().includes(q)), [connected, workflows, q]);

  if (open?.kind === 'demo') return <PlanView key="demo" demo onBack={() => setOpen(null)} prompt={prompt} />;
  if (workflow) return <PlanView key={workflow.id} workflow={workflow} onBack={() => setOpen(null)} prompt={prompt} />;

  return (
    <div className="wf-home">
      <div className="wf-hero">
        <h1 className="wf-display">What should your team do?</h1>
        <p className="wf-small">Describe it in plain words. MAOF drafts the steps, and you stay in control.</p>
        <div className="wf-prompt">
          <textarea aria-label="Describe the work" rows={2} placeholder={DEMO_PROMPT} value={prompt} onChange={e => setPrompt(e.target.value)} />
          <div className="wf-prompt-row">
            {EXAMPLES.map(text => <button key={text} type="button" className="wf-example" onClick={() => setPrompt(text)}>{text}</button>)}
            <button type="button" className="wf-btn wf-btn-primary wf-push" onClick={() => setOpen({ kind: 'demo' })} title="Plan generation is not connected to a model yet; this opens the sample plan">Generate plan</button>
          </div>
        </div>
        <div className="wf-row">
          <h2 className="wf-h3">Or start from a template</h2>
          {/* data-edit-workflow is handled by the classic editor, which owns every task field. */}
          <button type="button" className="wf-btn wf-btn-ghost wf-push" data-edit-workflow={connected ? 'new' : undefined}
            onClick={connected ? undefined : () => toast('Open the desktop app to create workflows.', true)}>Start blank</button>
        </div>
        <div className="wf-templates">
          {cards.map(card => (
            <button key={card.key} type="button" className="wf-template" data-workflow-card={card.key} onClick={card.open}>
              <span className="wf-template-name">{card.name}</span>
              <span className="wf-small">{card.detail}</span>
            </button>
          ))}
          {!cards.length && <p className="wf-small">{q ? `No workflows match “${query}”.` : 'No workflows yet. Start blank to create one.'}</p>}
        </div>
      </div>
    </div>
  );
}
