import type { RuntimeHealth, WriterHealth } from './types';

// Keep diagnostics collapsed; ordinary task actions remain on the run screen.
export function runtimeView(h: RuntimeHealth, w: WriterHealth, open: boolean): string {
 const item = (name: string, value: string|number) => `<div class="readiness-row"><span>${name}</span><strong>${value}</strong></div>`;
 return `<details class="panel runtime-health" ${open?'open':''}><summary>Engine health</summary>
 ${item('Engine',h.driver_running?'Running':'Stopped — reopen the app to recover')}
 ${item('Workers',`${h.active_workers} / ${h.result_capacity}`)}
 ${item('Queued changes',`${h.commands_pending} / ${h.command_capacity}`)}
 ${item('Queued controls',`${h.controls_pending} / ${h.control_capacity}`)}
 ${item('Pending results',`${h.results_pending} / ${h.result_capacity}`)}
 ${item('Request bytes in flight',`${h.command_bytes_in_flight} / ${h.command_byte_capacity}`)}
 ${item('Requests refused before acceptance',h.rejected_commands)}
 ${item('Worker failures handled',h.worker_failures)}
 ${item('Capacity waits observed',h.transport_waits)}
 ${item('Checkpoint writer',w.busy?'Saving or exporting':'Idle')}
 ${item('Queued checkpoints',`${w.commits_pending} / ${w.commit_capacity}`)}
 ${item('Queued exports',`${w.exports_pending} / ${w.export_capacity}`)}
 ${item('Exports preparing, queued or writing',`${w.exports_in_flight} / ${w.export_in_flight_limit}`)}
 ${item('Checkpoints saved this session',w.committed_checkpoints)}
 ${item('Writer failures',w.failures)}
 ${item('Exports refused before acceptance',w.rejected_exports)}
 ${item('Last checkpoint duration',`${w.last_commit_ms} ms`)}
 <p class="hint">These counters describe this app session. A full queue refuses the request. A lost reply may follow a saved change; check the run before retrying. Controls have reserved space and wait for the current transaction to finish.</p>
 </details>`;
}
