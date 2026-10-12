# CrewAI, LangGraph සහ AutoGen — orchestration, communication සහ runtime comparison

> Scope update (2026-10-12): the five-track [research scope](RESEARCH_SCOPE.md) and [roadmap](ROADMAP.md) supersede older full-product plans. This guide retains its original review/test dates.

Official sources පරීක්ෂා කළ දිනය: **2026-10-11**, Asia/Colombo.
මේක documentation සහ තෝරාගත් source-code review එකක්. Frameworks install කරලා
benchmark කළ report එකක් නොවේ. Sinhala explanations තුළ API/class names English
වලින් තබා තිබෙනවා. Performance, interoperability හෝ novelty මෙයින් prove නොවේ.

## කියවන්න පටන් ගන්න තැන

| File | පැහැදිලි කරන දේ |
|---|---|
| [CrewAI](34-crewai-orchestration-runtime-si.md) | Agents/Tasks/Crews/Flows, delegation, async execution, checkpointing සහ hosted boundaries |
| [LangGraph](35-langgraph-orchestration-runtime-si.md) | StateGraph, channels/reducers, Pregel super-steps, interrupts, checkpoints සහ retries |
| [AutoGen](36-autogen-orchestration-runtime-si.md) | Core/AgentChat/Extensions/Studio, messages, group-chat managers, runtime queue සහ distributed workers |

CrewAI sources මේ review එකේ **v1.15.27** documentation වෙත redirect වුණා.
LangGraph Python documentation සහ AutoGen **stable Core/AgentChat architecture**
භාවිත කළා; ඒ දෙකේ exact installed package version එකක් මෙහි claim කරන්නේ නැහැ.
Source code `main` හෝ generated stable module pages mutable නිසා benchmark එකකට
වෙනම package versions/commit IDs pin කරන්න. AutoGen 0.2 examples සහ මෙම
Core/AgentChat API එක mix නොකරන්න.

## “Orchestrator” සහ “runtime” දෙකේ වෙනස

**Orchestration** තීරණය කරන්නේ කුමන task/agent/node එක ඊළඟට ක්‍රියා කළ යුතුද,
කවදා dependencies සම්පූර්ණද සහ කවදා workflow නවත්වන්නේද කියන එකයි.
**Communication** තීරණය කරන්නේ inputs, outputs, status සහ context ගෙනයන්නේ
කොහොමද කියන එකයි. **Runtime** එම තීරණ execute කරන event loop/workers,
concurrency, errors, cancellation සහ lifecycle පාලනය කරන layer එකයි.

මේ terms frameworks තුනම එකම abstraction level එකේ භාවිත කරන්නේ නැහැ.
Agent conversation turn, graph super-step සහ model-call dispatch එක සමාන units
ලෙස benchmark කළොත් misleading results ලැබිය හැකියි.

## එකම dimensions මත comparison එක

පහත rows source-backed summaries. Detail සහ sources අදාළ framework guide තුළයි.

| Dimension | CrewAI | LangGraph | AutoGen |
|---|---|---|---|
| Main abstraction | Role-based Agent + Task; Crew process; outer Flow | Shared state + nodes + edges compiled into graph runtime | Core message-handling agents; AgentChat teams on top |
| Next work selection | Sequential task order හෝ hierarchical manager; Flow conditions | Edges/routes සහ updated channels activate nodes | Round-robin, model/custom selector, handoff; graph presetත් ඇත |
| Data/context transfer | Task output/context; coworker delegation tools; Flow state | Channel/state updates combined by reducers | Direct request/response හෝ topic publish/subscribe; team conversation events |
| Concurrency unit | Async tasks, native async crew execution, Flow branches | Selected nodes in a super-step; configurable concurrency | Concurrent async message handling; selected team preset may serialize turns |
| Persistence | Configured Crew/Agent checkpointing සහ Flow persistence | Configured checkpointer/thread; cross-thread Store වෙනම | Explicit agent/team/runtime state save/load; app must persist returned state |
| Human intervention | Task human input, Flow feedback; hosted HITL API වෙනම | Persistent interrupt + resume value | User interaction, handoff/termination and team continuation |
| External integration | A2A remote agents; MCP tools | Node can invoke external clients/services; deployment separately configured | Extensions/model clients/tools; distributed runtime host/workers |
| Product boundary | Python library ≠ hosted AMP features | Graph library ≠ Agent Server/LangSmith deployment | Core/AgentChat ≠ Studio UI ≠ distributed deployment |

References: [CrewAI processes](https://docs.crewai.com/v1.15.27/en/concepts/processes),
[LangGraph Graph API](https://docs.langchain.com/oss/python/langgraph/graph-api),
[AutoGen teams](https://microsoft.github.io/autogen/stable/user-guide/agentchat-user-guide/tutorial/teams.html).

## Same research workflow example

Use a research task: collect source notes, compare options, and prepare a report
after both prerequisites finish. CrewAI can express this through Tasks/context,
LangGraph through nodes/edges/state, and AutoGen through messages and a team manager.
This is a proposed mapping, not an executed cross-framework experiment.

## ඔබේ Rust/Tauri project එකට mapping — අපේ analysis

Current implementation references: [implementation guide](../README.md),
[runtime](29-bounded-runtime-si.md), [writer](30-async-durable-writer-si.md).
මෙහි implementation status එක current project records අනුවයි; අලුත් live tests
මෙම documentation task එකේ run කරලා නැහැ.

| Our module | Reference design idea | දැනට අපේ implementation සහ gap |
|---|---|---|
| Step coordinator | Crew process / graph control / team manager | Fixed dependency templates, deterministic capability assignment, four feasible-task policies; general conditional loops නැහැ |
| Transport | Typed Core messages / graph updates | Bounded command/control queues, byte admission, reserved result capacity; arbitrary agent pub/sub bus නොවේ |
| Workers | Async node/handler execution | Per-agent serialization, bounded slots, panic/deadline/missing-result settlement; OS process sandbox හෝ preemptive model scheduler නොවේ |
| Durable writer | Checkpoints/state snapshots | Save-before-publish/ack with owned transaction gate; append-only execution journal නොවේ |
| Budgets | Crew rate/iteration controls; AutoGen usage termination | Shared/per-agent reservation and confirmed/uncertain accounting; daily billing ledger නැහැ |
| Tools/policy | Guardrails, interrupts, intervention hooks | Concrete document-read/report approvals; generic external mail/calendar effect approval adapter තව නැහැ |
| External agents | A2A / custom runtime gateways | HTTP JSON + A2A v0.3 text/polling subset; unrestricted compatibility claim නැහැ |

මේ mapping එකෙන් “අපේ engine එකට අනෙක් සියල්ලේ features තියෙනවා” කියලා
කියන්නේ නැහැ. Conversation selection, channel updates සහ runtime admission
වෙනස් problems විසඳනවා. Shared state තිබීමෙන් authorization ලැබෙන්නේ නැහැ;
checkpoint එකක් තිබීමෙන් remote side effect exactly once වීමත් guarantee නොවේ.

## Fair comparison සහ research gap එක තහවුරු කරන විදිහ

Reviewed documentation තුළ application orchestration/control features ඇත.
මේ pagesවල අපේ exact priority-aging + RAM planning + uncertain-usage ledger
combination එක හඳුනාගත්තේ නැහැ. **මෙය review finding එකක් මිස entire ecosystems
තුළ එය නැති බවට proof එකක් නොවේ.** Custom nodes/managers/extensions මගින්
මෙවැනි policies implement කළ හැකි නිසා baseline configuration සඳහන් කළ යුතුයි.

1. Same task DAG, models, prompts, tool responses, hardware සහ service delays තබන්න.
2. Framework-native defaults සහ equivalent custom admission policy වෙන වෙනම compare කරන්න.
3. Manager/selector LLM calls ඇතුළු සියලු orchestration tokens/calls count කරන්න.
4. Completion, ready/resource/dependency waits, latency, failure handling සහ unknown effects record කරන්න.
5. Duplicate results, timeout-after-server-accept, crash-before-save සහ approval revoke scenarios inject කරන්න.
6. Synthetic traces, live-model results සහ actual account effects වෙන වෙනම label කරන්න.

Existing experiment plan: [controlled evaluation](RESEARCH_SCOPE.md),
[scheduler research increment](RESEARCH_SCOPE.md).

## Supervisor discussion සඳහා සරල explanation එක

CrewAI role/task collaboration පහසු කරනවා. LangGraph explicit state/control
flow සහ replay/recovery boundaries model කරනවා. AutoGen messages, teams සහ
agent-runtime separation පැහැදිලි කරනවා. අපේ research question එක constrained
host එකක admission, fair ordering සහ uncertain usage එකට පාලනය කිරීම useful
completion සහ wait distribution වෙනස් කරනවාද කියන එකයි. ඒක measurable
hypothesis එකක්; මෙම source review එක comparative performance result එකක් නොවේ.

## Evidence status සහ maintenance

| Evidence | මෙම task එකේ තත්ත්වය |
|---|---|
| Official documentation | Sources open කර mechanisms පරීක්ෂා කළා; individual guidesහි claim-level links ඇත |
| Source inspection | AutoGen generated runtime module; CrewAI public Crew definition පමණක් තෝරාගත් review |
| Installed versions / live framework tests | Run කරලා නැහැ |
| Cross-framework benchmark / novelty proof | තවම නැහැ |

AutoGen official repository මේ review දිනට **maintenance mode** බවත් new projects
සඳහා Microsoft Agent Framework බලන්න කියන බවත් සඳහන් කරනවා. AutoGen architecture
comparison එක මෙහි තවම අදාළයි; migration target එක මෙම scope එකේ හතරවැනි
framework ලෙස analyze කළේ නැහැ. [Official repository](https://github.com/microsoft/autogen).
