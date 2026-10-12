# Five-track research scope

Revision: **2026-10-12**, Asia/Colombo. These are candidate contributions and research
questions, not proven novelty, token savings or framework superiority.

## R1 — Dependency- and resource-aware scheduling with priority support

Select eligible steps using the configured policy after dependency, agent, worker,
declared/available memory and shared/per-agent budget checks. Compare FIFO, Round
Robin, strict priority and dispatch-based aging on identical inputs. Aging cannot
override a failed dependency or impossible resource requirements.
The current task priority comes from its assigned agent; an independently editable
per-task priority field is not implemented.

Measure completion/rejection, first response, turnaround, and ready/resource/
dependency waits separately. Resource-aware agent systems already exist, including
[AIOS](https://github.com/agiresearch/AIOS); compare matched admission policies and
include an aging ablation.

## R2 — Task recovery and cancellation

Recover saved work and supported remote IDs; inspect an existing remote task before
resubmitting it. Cancellation is best effort and unresolved status/usage remains
uncertain. Measure duplicate settlement, resubmission, crash and timeout handling.

Recovery/cancellation are already in the
[A2A specification](https://a2a-protocol.org/v0.3.0/specification/). The proposed
contribution integrates them with admission and uncertainty accounting. Saved
snapshots and local fences cannot guarantee remote exactly-once effects.

## R3 — Visual debugger for agent handoffs

Plan a causal view of dispatch payloads, dependency context, results, decisions and
failure labels. Read-only replay inspection must not invoke effects. Message edits
fork a new run, preserving original history and rechecking permissions/approvals.

[MAST](https://arxiv.org/abs/2503.13657) studies 1600+ traces across seven systems,
with system-design, inter-agent-misalignment and verification categories. Its
failure rates concern evaluated systems/tasks, not all deployed agents, and do not
establish that most failures happen at communication boundaries. These rates are
not performance results for MAOF_G4.

[LangGraph time travel](https://docs.langchain.com/oss/python/langgraph/use-time-travel)
already supports replay and modified-state forks. Our candidate angle is a local
handoff debugger exposing admission/usage decisions with failure labels. Measure
fault-localization time, classification accuracy and replay fidelity.

## R4 — Dynamic tool selection for token efficiency

Start with keyword ranking, then an optional vector baseline. Select schemas using
current task/turn and observations after permission filtering. Evaluate configurable
top-k (including 2 and 3) and schema budgets, required-tool retention and an explicit
no-match/fallback path. Selection needs a live structured model/tool execution loop.

Related mechanisms already exist in
[LangChain tool-selector/search middleware](https://docs.langchain.com/oss/python/langchain/middleware/built-in)
and [CrewAI MCP filtering](https://docs.crewai.com/en/mcp/overview). Do not claim
those frameworks always inject every schema. Compare all authorized schemas,
keyword top-k and vector selection on identical catalogs/tasks. Measure latency,
schema bytes/tokens, required-tool recall, task success and total usage. Millisecond
latency and token/quality benefits remain hypotheses until measured.

## R5 — Engine-mediated communication and message optimization

Workers return results, status, errors and usage to the engine. The engine forwards
bounded context to declared dependencies; workers do more than status-only reporting.
Transport is bounded with reserved control capacity. This is not a new protocol.

Central coordination already exists in
[CrewAI](https://docs.crewai.com/en/concepts/processes),
[LangGraph](https://docs.langchain.com/oss/python/langgraph/graph-api) and
[Orkas](https://github.com/Orkas-AI/Orkas). Not all frameworks use all-to-all chat.
Compare dependency-scoped context with explicitly constructed broadcast/full-history
baselines. Separate control messages from text actually sent to a model: fewer
messages do not automatically mean fewer billed tokens.

Measure counts/bytes, context tokens, quality, overhead and control admission/ACK
latency under overload.

## Evaluation contract

Use identical DAGs, tasks, models, prompts, hardware and service behavior. Record
framework/package versions, dataset, seed and trial count. Count routing, selection,
summarization and retries. Label Demo, HTTP fixtures and live observations separately.
Docs/source reviews describe mechanisms; they do not establish benchmark results.
