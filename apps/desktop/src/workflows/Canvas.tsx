import { useState } from 'react';
import type { Plan, RunView, StepStatus } from './model';

// Geometry from the "05-Workflow / Canvas" frame.
const NODE_W = 112, NODE_H = 64, COL = 128, ROW = 98, PAD = 15, MIN_H = 440;
const STATUS_TONE: Partial<Record<StepStatus, string>> = { done: 'completed', running: 'running', approval: 'paused', failed: 'failed', queued: 'queued' };

type Node = { id: string; x: number; y: number; owner: string; title: string; kind: 'trigger' | 'agent' | 'decision'; deps: string[] };

function layout(plan: Plan) {
  const columns: Omit<Node, 'x' | 'y'>[][] = [
    [{ id: '__trigger', owner: 'Trigger', title: plan.trigger.title, kind: 'trigger', deps: [] }],
    ...plan.stages.map(stage => stage.steps.map(s => ({ id: s.id, owner: s.kind === 'decision' ? 'Decision' : s.owner, title: s.title, kind: s.kind, deps: s.dependencies.length ? s.dependencies : ['__trigger'] }))),
  ];
  const tallest = Math.max(...columns.map(c => c.length));
  const height = Math.max(MIN_H, tallest * ROW + 2 * PAD);
  const mid = height / 2;
  const nodes: Node[] = columns.flatMap((col, c) => col.map((n, j) => ({ ...n, x: PAD + c * COL, y: mid + (j - (col.length - 1) / 2) * ROW - NODE_H / 2 })));
  return { nodes, width: PAD * 2 + columns.length * COL - (COL - NODE_W), height };
}

export function Canvas({ plan, run, selected, onSelect }: { plan: Plan; run?: RunView; selected?: string; onSelect: (id: string) => void }) {
  const [zoom, setZoom] = useState(1);
  const { nodes, width, height } = layout(plan);
  const byId = new Map(nodes.map(n => [n.id, n]));
  const edges = nodes.flatMap(n => n.deps.map(d => byId.get(d)).filter((s): s is Node => !!s).map(s => ({ from: s, to: n })));
  const loopFrom = plan.loop && byId.get(plan.loop.from), loopTo = plan.loop && byId.get(plan.loop.to);
  const clampZoom = (z: number) => Math.min(1.5, Math.max(0.5, Math.round(z * 10) / 10));

  return (
    <div className="wf-card wf-canvas" style={{ height: Math.max(MIN_H, height) }}>
      <div className="wf-canvas-scroll">
        <div style={{ width: width * zoom, height: height * zoom }}>
          <div className="wf-canvas-inner" style={{ width, height, transform: `scale(${zoom})` }}>
            <svg className="wf-edges" width={width} height={height} aria-hidden="true">
              {edges.map(({ from, to }) => {
                const x1 = from.x + NODE_W, y1 = from.y + NODE_H / 2, x2 = to.x, y2 = to.y + NODE_H / 2, c = (x2 - x1) / 2;
                return (
                  <g key={`${from.id}-${to.id}`}>
                    <path d={`M${x1} ${y1} C${x1 + c} ${y1} ${x2 - c} ${y2} ${x2} ${y2}`} />
                    <circle cx={x2} cy={y2} r={3.5} />
                  </g>
                );
              })}
              {loopFrom && loopTo && (
                <path className="wf-loop" d={`M${loopFrom.x + NODE_W / 2} ${loopFrom.y + NODE_H} C${loopFrom.x + NODE_W / 2} ${loopFrom.y + NODE_H + 45} ${loopTo.x + NODE_W / 2} ${loopTo.y + NODE_H + 45} ${loopTo.x + NODE_W / 2} ${loopTo.y + NODE_H}`} />
              )}
            </svg>
            {loopFrom && loopTo && <span className="wf-loop-label" style={{ left: loopTo.x + NODE_W / 2 + 4, top: loopTo.y + NODE_H + 44 }}>{plan.loop!.label}</span>}
            {nodes.map(n => {
              const tone = n.kind === 'trigger' ? (run ? 'completed' : undefined) : STATUS_TONE[run?.steps[n.id] ?? 'planned'];
              const clickable = n.kind !== 'trigger';
              return (
                <div key={n.id} className={`wf-cnode${n.kind === 'decision' ? ' decision' : ''}${selected === n.id ? ' selected' : ''}${tone ? ` tone-${tone}` : ''}`}
                  style={{ left: n.x, top: n.y }} role={clickable ? 'button' : undefined} tabIndex={clickable ? 0 : undefined}
                  onClick={clickable ? () => onSelect(n.id) : undefined}
                  onKeyDown={clickable ? e => { if (e.key === 'Enter') onSelect(n.id); } : undefined}>
                  <span className="wf-cnode-owner">{n.owner}</span>
                  <span className="wf-cnode-title">{n.title}</span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
      <div className="wf-zoom" role="group" aria-label="Zoom">
        <button type="button" aria-label="Zoom out" onClick={() => setZoom(z => clampZoom(z - 0.1))}>−</button>
        <span>{Math.round(zoom * 100)}%</span>
        <button type="button" aria-label="Zoom in" onClick={() => setZoom(z => clampZoom(z + 0.1))}>+</button>
      </div>
      <svg className="wf-minimap" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="xMidYMid meet" aria-hidden="true">
        {nodes.map(n => <rect key={n.id} x={n.x} y={n.y} width={NODE_W} height={NODE_H} rx={10} />)}
      </svg>
    </div>
  );
}
