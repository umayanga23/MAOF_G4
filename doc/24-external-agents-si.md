# External agents සම්බන්ධ කරන හැටි

> Scope update (2026-10-12): the five-track [research scope](RESEARCH_SCOPE.md) and [roadmap](ROADMAP.md) supersede older full-product plans. This guide retains its original review/test dates.


Lecturerගේ academic/mail/calendar external agents සඳහා executable setup,
free-resource links සහ real-model/account testing steps
[guide 31](../README.md) තුළ තියෙනවා.

Implementation date: **2026-10-06**. Agent එකකට Local Ollama, External HTTP JSON
හෝ External A2A JSON-RPC connection එකක් තෝරන්න පුළුවන්. External agents පමණක්
තියෙන workflow එකකට Ollama install කරලා තිබීම අවශ්‍ය නැහැ.

## 1. දැනට සම්බන්ධ කළ හැකි agent වර්ග

| Connection | අවශ්‍ය interface එක |
|---|---|
| Local Ollama | Installed model + Settings හි local Ollama address |
| External HTTP JSON | පහත `orchestration-http-v1` request/response contract එක පිළිගන්න POST endpoint එකක් |
| External A2A JSON-RPC | A2A v0.3 JSON-RPC `message/send` සහ අවශ්‍ය නම් `tasks/get` පිළිගන්න execution endpoint එකක් |

External agent එකේ model/provider, tools සහ account access එහි service එකේම
තියෙනවා. මේ app එක task submission, bounded polling, result acceptance,
dependencies, priority, local context සහ local report approval coordinate කරනවා.
Raw OpenAI/Claude model endpoint එකක්, ChatGPT share link එකක් හෝ agent-card URL
එකක් execution endpoint field එකට දැමීමෙන් සම්බන්ධ වෙන්නේ නැහැ.

A2A ක්‍රමය [official v0.3 specification](https://a2a-protocol.org/v0.3.0/specification/)
අනුව text/polling subset එකක්. මෙය full A2A compliance claim එකක් නොවේ.
[CrewAI delegation](https://docs.crewai.com/en/learn/a2a-agent-delegation),
[Microsoft remote-agent wrapper](https://learn.microsoft.com/en-us/agent-framework/integrations/by-component/agent-services/a2a)
සහ [LangGraph Agent Server](https://docs.langchain.com/langsmith/server-a2a)
සඳහා reference documentation බැලුවා. ඒ frameworks install කරලා interoperability
benchmark කිරීම තවම pending; ඔයාගේ server/version එක මේ subset එකට match විය යුතුයි.

## 2. Ollama නැතිව ඉක්මන් demonstration එක

Project root එකේ terminal එකක් open කරලා:

```powershell
Set-Location F:\multi-agent-orchestrator
python examples/external_agent_server.py
```

මේ terminal එක open තබන්න. Service එක `http://127.0.0.1:8765/run` සහ `/a2a`
endpoints දෙකම සපයනවා. **මෙය synthetic agent service එකක්**: AI model, email
account හෝ stock-data feed එකක් call කරන්නේ නැහැ. Output එකේ `EXTERNAL SAMPLE
AGENT` label එක තියෙනවා; remote connection පෙන්වීමටයි.

වෙනත් terminal එකක fresh practice workspace එක සකස් කරන්න:

```powershell
Set-Location F:\multi-agent-orchestrator
cargo build -p api --examples --locked
python doc/tools/prepare_external_demo.py
```

Script එක print කරන `$env:ORCHESTRATION_LAB_DATA_DIR = '...'` සහ executable
command දෙක **එම terminal එකේ** run කරන්න. Personal workspace එක edit කරන්නේ
නැහැ. Each setup creates a fresh `.cache/external-demo-...` directory.

A2A connection එකෙන් වෙනම practice workspace එකක් හදන්න:

```powershell
python doc/tools/prepare_external_demo.py --a2a
```

App එකේ **Overview → External agent team**, prompt:

```text
Compare the supplied Orchard approaches and preserve the SAMPLE42 project marker.
```

1. **Start workflow** කරන්න.
2. Approvals → synthetic `feature-lab-notes.md` document read එක review කරලා allow කරන්න.
3. External Researcherගේ result එක External Writerට dependency context ලෙස යනවා.
4. Approvals → Writer report preview review කරලා **Allow this report** කරන්න.
5. Run history → outputs, decisions, usage → **Export JSON** කරන්න.

Successful sample run එකේ agent calls **2**, document reads **1**, report **1**
සහ Completed status බලාපොරොත්තු වෙනවා. A2A polling HTTP requests වෙනම model/agent
calls ලෙස ගණන් නොකරයි; delegation attempt එකකට එක call count එකක් පමණයි.
Sample service එක model call නොකරන නිසා reported tokens **0** යි.

## 3. UI එකෙන් ඔයාගේ external agent එක register කිරීම

**Agents → Add agent**:

| Field | දෙන්න ඕනේ දේ |
|---|---|
| Name / Role | උදාහරණය `External Researcher` සහ එයා කරන වැඩේ |
| Agent connection | External HTTP JSON හෝ External A2A JSON-RPC |
| External agent execution URL | Full endpoint, උදාහරණය `https://agent.example.com/a2a` |
| Bearer token environment variable | අවශ්‍ය නම් token එක තියෙන variable නම; actual token value එක නොවේ |
| External timeout | සම්පූර්ණ operation එකට 1–120 seconds; A2A submission + polling එකටත් ඒ එකම deadline එක |
| Send task context to this external service | Document/previous-step/dependency context යවන්න අවශ්‍ය නම් enable කරන්න; default Off |
| Priority | 1 highest, 9 lowest; ready tasks අතර preference එක |
| Declared memory / Token budget | Local coordinator planning/admission limits; remote machine RAM හෝ billing enforce කරන්නේ නැහැ |
| Instructions | Remote serviceට task guidance; එහි trusted policy වෙනස් කරන permission එකක් නොවේ |

**Save agent → Workflows → New workflow/Edit workflow → Task Agent dropdown**
එකෙන් agent select කරන්න. මුලින් steps **1**, document None, dependency field
හිස් සහ report checkbox Off තබලා කුඩා connection test එකක් කරන්න.

**Settings → Execution mode → Live — local models and external agents** save
කරලා අලුත් run එකක් start කරන්න. Demo mode එකෙන් external agentsත් simulate
කරනවා; කිසිම external endpoint එකක් contact කරන්නේ නැහැ. Backward compatibility
සඳහා JSON settings වල live mode value එක තවම `ollama`යි. External agent routing
තීරණය කරන්නේ pinned agent configuration එකෙන්; Ollama address එක භාවිතා නොවේ.

External Researcher සහ local Ollama Writer එකම workflow එකේ mix කළ හැකියි.
Local agent භාවිතා කරන branch එකට පමණක් Ollama අවශ්‍යයි.

## 4. Credentials

App එක start කරන PowerShell session එකේ variable එක set කරන්න:

```powershell
$env:ORCHESTRATION_AGENT_STOCK_TOKEN = '<your service bearer token>'
& 'F:\multi-agent-orchestrator\target\debug\orchestration-desktop.exe'
```

UI field එකට `ORCHESTRATION_AGENT_STOCK_TOKEN` නම පමණක් දෙන්න. Variable නම
`ORCHESTRATION_AGENT_` prefix සහ uppercase letters/digits/underscores භාවිතා කළ
යුතුයි. Actual value backend එකෙන් read කරනවා; checkpoint/export/UI තුළ save
කරන්නේ නැහැ. Already-running app එකට නව terminal variable එකක් ලැබෙන්නේ නැහැ;
credentials set කළ terminal එකෙන් app එක නැවත start කරන්න.

Remote URLs සඳහා HTTPS අවශ්‍යයි. HTTP පිළිගන්නේ localhost/loopback සඳහා පමණයි.
URL credentials, query strings සහ fragments reject කරනවා. Redirects සහ ambient
proxy use disabledයි; credentials වෙනත් destination එකකට redirect නොවේ.

## 5. Custom HTTP contract

POST body:

```json
{
  "protocol_version": "orchestration-http-v1",
  "request_id": "unique-attempt-id",
  "run_id": "local-run-id",
  "task_id": "research",
  "step": 1,
  "agent": {"id": "external-researcher", "version": 1},
  "instructions": "Explain the supplied topic",
  "prompt": "User request and assigned task",
  "context": "Bounded context, or empty when sharing is disabled",
  "limits": {"output_tokens": 128, "reserved_tokens": 2048}
}
```

Successful HTTP 200 JSON response:

```json
{
  "request_id": "unique-attempt-id",
  "output": "The external agent's result",
  "usage": {"tokens": 123}
}
```

`request_id` must echo the request. `output` nonempty text විය යුතුයි. Usage
නොදන්නා විට `usage` omit කරන්න හෝ `tokens: null` යවන්න; zero invent කරන්න එපා.
Reported token count unsigned integer 0–10,000,000 තුළ විය යුතුයි. Request limits
remote agentට hints; ඒ service එක ඒවා enforce කරන බව මේ app එකට prove කරන්න බැහැ.

Existing Python/JS agent එකක් මේ contract එකට wrap කරන්න පුළුවන්. Included sample
service එකේ response function වෙනුවට ඔයාගේ agent's API/SDK call එක යොදන්න.
Cloud credentials සහ email/market tool credentials remote service එකේ තබන්න.

## 6. A2A subset සහ runtime limits

- Direct configured JSON-RPC endpoint හෝ [reviewed Agent Card discovery](26-agent-discovery-si.md).
  Network scans, registry search සහ automatic card refresh නොමැත.
- `message/send` සඳහා durable submission intent; submitted/working tasks සඳහා
  saved remote task ID එකට `tasks/get` polling. Restart/resume අලුත් submission
  එකක් නොකරයි. [Recovery guide](25-external-task-recovery-si.md) බලන්න.
- Immediate agent Message text, completed Task text artifacts හෝ agent status-message
  text භාවිතා කරනවා. File URLs fetch හෝ data-parts/tool requests execute නොකරයි.
- Standard A2A token usage field එකක් assume නොකරයි. Optional project convention:
  response `metadata.orchestration_usage.tokens`; otherwise unknown usage retained.
- `input-required` / `auth-required` explicit unsupported outcome එකක්. Interactive
  continuation, SSE, gRPC, HTTP+JSON binding, callbacks සහ v1-only methods නොමැත.
- Polling එකක worker slot සහ planning reservation තබනවා. Response එකකට 256 KiB
  cap; accepted task output එක existing 16,000-character cap තුළයි.
- Stop final submission gateට කලින් accepted නම් queued request නොයවයි.
  Gate පසු remote work in flight විය හැකියි. Saved task ID සඳහා best-effort
  `tasks/cancel`; confirmation/state UI එකේ වෙනම පෙන්වනවා.
- A2A timeout/connection/protocol errors පසු identity සහ uncertain usage රඳවලා
  run pause කරනවා. Resume status පරීක්ෂා කරනවා; automatic resubmission නොවේ.
  Simple HTTP JSON errors සඳහා පෙර failure semantics එලෙසමයි.
- Remote task ID, step සහ cancel intent checkpoint වෙනවා. Late terminal usage
  එක්වරක් settle කරනවා. Usage නොදෙන්නේ නම් uncertainty රැඳෙනවා. Task ID නොලැබුණු
  unknown submission එක manual service review අවශ්‍යයි; blind replay නොකරයි.
  Exactly-once external execution හෝ side-effect rollback guarantee නොකරයි.

මේ connectors remote model, internal tools, credentials හෝ OS resources පාලනය
කරන්නේ නැහැ. Local report/document approval එක external service එකේ tools සඳහා
approval grant එකක් නොවේ. External output එක existing bounded context ලෙසයි;
permissions හෝ configurations වෙනස් කරන්න බලයක් නැහැ.

## 7. Validation

Executed results [verification record](VERIFICATION.md) හි dated
section එකෙන් බලන්න. New checks cover HTTP authentication/correlation/context,
A2A polling/artifacts/states, unknown usage, timeout, endpoint validation, queued
stop and pinned config, plus two real-service browser workflow scenarios.

Commands (Cargo build finish වූ පසු UI test කරන්න):

```powershell
cargo test --workspace --locked
cargo build -p api --examples --locked
python -m py_compile examples/external_agent_server.py doc/tools/prepare_external_demo.py
python doc/tools/check_project_docs.py
Set-Location apps/desktop
npm.cmd run build
npm.cmd run test:ui
npm.cmd run tauri build -- --debug --no-bundle
npm.cmd run test:native
npm.cmd run test:external-native
npm.cmd run test:remote-native
npm.cmd run test:discovery-native
```

Manual test එකක endpoint/protocol/version, agents, context-sharing setting,
status, reported/unknown usage සහ export path record කරන්න. Actual provider
agent එකක් connect කරලා output quality සහ interoperability වෙනම evaluate කරන්න.

## 8. Architecture එකට ගැළපෙන implementation එක

Final diagrams වල **Agent and task setup → Orchestration engine → Agent runtime
→ Model and tool adapters → Outside agent service** path එක භාවිතා කරනවා.
Remote agent එකේ අභ්‍යන්තර tools අපේ local permission module එකෙන් execute
කරන්නේ නැහැ; එහි service policy එකෙන් පාලනය විය යුතුයි.

| Project file | වගකීම |
|---|---|
| [Agent types](../crates/domain/src/prototype.rs) | Connection protocol, endpoint, timeout, context consent සහ token-variable name; old checkpoints සඳහා default |
| [External adapter](../crates/llm/src/external.rs) | Bounded HTTP request/response, authentication සහ A2A polling |
| [Runtime routing](../crates/llm/src/local.rs) | Demo simulation හෝ pinned agent අනුව local/external execution |
| [Desktop service](../crates/api/src/desktop.rs) / [A2A lifecycle](../crates/api/src/remote.rs) | Durable dispatch/remote identity, stop/state gates සහ restart/cancel/status observation |
| [Coordinator](../crates/orchestrator/src/prototype/mod.rs) | Existing priority, admission, dependencies, completion සහ uncertain-usage settlement |
| [Desktop UI](../apps/desktop/src/main.ts) | Agent connection editor සහ Live mode |
| [Sample agent service](../examples/external_agent_server.py) | Ollama නැතිව HTTP/A2A integration පෙන්වන independent synthetic service |

අලුත් external service එකක් connect කළත් task run එකේ agent version/config
immutable snapshot එකක් ලෙස තබනවා. පසුව Agents screen එකේ endpoint edit කළාට
දැනට ක්‍රියාත්මක run එක වෙනත් service එකකට මාරු වෙන්නේ නැහැ.
