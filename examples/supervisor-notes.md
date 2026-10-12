# Supervisor review notes

Prepare a research progress review for the current resource-aware multi-agent
orchestrator, its implemented behavior and the next development increments.

Describe the Rust/Tauri desktop architecture: client/API, agent/task setup,
orchestration, worker runtime, model/tool adapters, resource/token admission,
permissions, wait/failure handling, memory, observability and durable state.

Cover local Ollama roles and external HTTP/A2A agent services. Explain Agent Card
review, capability matching, deterministic priority/ID selection, pinned agent
versions, task dependencies and the difference between assignment and scheduling.
Include a mixed local/external workflow and review how the platform controls its
own operations while remote tools and account access follow the remote service's
authorization. Email triage, search and market research are potential application
workloads, not project exclusions or already measured live integrations.

Compare FIFO, Round Robin, strict priority and priority aging on identical inputs.
Report completion counts and ready, resource and dependency waits separately.
Explain when a blocked low-priority task is served and what happens after a cycle.

Demonstrate document/report approval and rejection, shared/per-agent budgets,
private/dependency context, pause/resume/stop, checkpoint reopening, saved-ID A2A
recovery and confirmed versus uncertain cancellation. Evaluate duplicate/stale
results and permission changes as well as successful runs.

Distinguish completed features from the remaining roadmap: MCP/tool registry,
conditional workflows/review loops, streaming, visual workflow editing, long-term
retrieval, bounded communication and stronger recovery/fault supervision.
Identify the acceptance test and evidence needed for each next increment.

Keep scripted demonstration output separate from real local-model observations.
Record hardware, model version, dataset, seed and trial count for later evaluation.
Do not claim a performance improvement before the controlled experiments.
