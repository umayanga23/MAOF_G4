# Feature lab source notes

This is synthetic test data, not measured research evidence.

Project marker: SAMPLE42
Project name: Orchard
Goal: coordinate local and independently hosted AI agents through a Rust/Tauri desktop application.
Approach A: FIFO scheduling is straightforward but does not favour urgent tasks.
Approach B: priority scheduling favours urgent tasks but can delay low-priority work.
Possible mitigation: priority aging gradually improves waiting tasks' priority.

## Platform workflow

Register local Ollama roles or external HTTP/A2A services. Review an external
Agent Card, configure its connection and credentials, and give each agent
user-reviewed capability tags, priorities, declared RAM and token budgets.
Assign tasks manually or match every required capability, then pin the selected
agent configuration for the run. Coordinate dependencies and bounded work steps
under shared resource and usage admission.

Document reads and report saves use concrete deny/ask/allow policies. Record
task states, waits, decisions, outputs and confirmed/uncertain usage. Preserve
private/shared task context, checkpoint progress, and recover saved remote task
identities without submitting the same task again. Distinguish a requested stop
from a confirmed remote cancellation. Detect supported wait cycles and show
recovery or escalation outcomes.

## Representative application scenarios

- Research: collect source material, compare approaches, review findings and
  prepare a report using local and external specialists.
- Email triage: delegate to an external agent service that already has authorized
  mailbox access; return a summary for another agent to review.
- Market research: delegate source retrieval to an external service connected to
  a data provider; pass dated, attributed results to analysis/report tasks.
- Search and database work: connect compatible external agent services now;
  direct MCP/tool registry integration is a planned platform extension.

These scenarios are not permanently excluded from the project. Account access,
available tools and data sources depend on the configured agent service or a
separately implemented platform adapter. A capability tag alone does not supply
those integrations. This fixture contains no real mailbox or market observations.

## Next development and evaluation

The roadmap includes tool registry/MCP support, conditional review loops, live
output streaming, a visual workflow editor and searchable long-term memory.
Compare FIFO, Round Robin, strict priority and aging on identical workloads,
separating ready/resource/dependency waits. Evaluate local and external execution,
approval/rejection, restart, uncertain cancellation and budget exhaustion.
Record actual observations separately from this synthetic source material.
