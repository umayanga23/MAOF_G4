# Agent Card එකෙන් external agent එකක් add කරන හැටි

> Scope update (2026-10-12): the five-track [research scope](RESEARCH_SCOPE.md) and [roadmap](ROADMAP.md) supersede older full-product plans. This guide retains its original review/test dates.


2026-10-07 increment: දැන් **Agents → Discover agent** මගින් A2A Agent Card URL
එකෙන් නම, description, skills, connection URL සහ authentication requirement
බලන්න පුළුවන්. Preview එක review කරලා **Use these details → Save agent** කළාම
registration වෙනවා. [External-agent guide](24-external-agents-si.md) සහ
[restart/cancellation guide](25-external-task-recovery-si.md) මේ flow එකටත් අදාළයි.

## 1. Sample service එකෙන් බලන්න

```powershell
cd F:\multi-agent-orchestrator
python examples/external_agent_server.py --port 8765
```

Service එක running තබලා updated `target/debug/orchestration-desktop.exe` open කරන්න.

1. **Agents → Discover agent** click කරන්න.
2. Agent Card URL ලෙස `http://127.0.0.1:8765/.well-known/agent-card.json` දාන්න.
   Sample එකේ card fetch සඳහා bearer variable අවශ්‍ය නැහැ.
3. **Fetch Agent Card** click කරන්න. `External sample researcher`, research skill,
   tags සහ `http://127.0.0.1:8765/a2a` execution URL පෙන්විය යුතුයි.
4. Details review කරන්න. Card එක වෙනත් service URL එකක් දක්වනවා නම් ඒකත්
   preview එකේ පෙන්වනවා; fetch කරන වෙලාවේ එයට request යවන්නේ නැහැ.
5. **Use these details** click කරන්න. Agent editor එකේ නම සහ A2A endpoint පිරෙනවා.
   Priority, memory සහ token budget අවශ්‍ය විදිහට සකසන්න.
6. Default document/report permissions **Deny**, context sharing **off**, execution
   credential **blank**. අවශ්‍ය protected action එකකට පමණක් Ask/Allow තෝරන්න.
   **Save agent** කළාම පමණක් workspace එකට agent එක එකතු වෙනවා.
7. Workflow එකකට එම agent තෝරන්න. මුලින් steps **1**, document/report නැති සරල
   task එකක් භාවිතා කරන්න. Settings එකේ **Live — local models and external agents**
   mode තෝරලා run කරන්න. Sample output scriptedයි; Ollama අවශ්‍ය නැහැ.

Saved agent එක Edit කළ විට reviewed metadata සහ advertised skills නැවත පෙන්වනවා.
Endpoint/protocol වෙනස් කරලා save කළොත් කලින් card metadata ඉවත් කරනවා. දැනට
run වන task එක launch වෙද්දී pin කළ agent/version/card metadata තබාගන්නවා.

## 2. Card URL සහ execution URL වෙනස් දේවල්

| URL | කරන දේ |
|---|---|
| Agent Card URL | `GET` මගින් service metadata කියවීම |
| Execution URL | Task එක launch කළ පසු A2A `message/send`, status සහ cancellation calls |

User දෙන **full card URL එක පමණක්** fetch කරනවා. Network scan, public registry
search හෝ chat link එකකින් agent සෑදීම මෙහි නැහැ. Well-known path එක A2A design
එකෙන් ගන්නවා. Card එකේ transport declarations අනුව JSONRPC primary endpoint
භාවිතා කරනවා; preferred transport වෙනත් එකක් නම් declared JSONRPC additional
interface එකක් තෝරනවා. [A2A v0.3 Agent Card specification](https://a2a-protocol.org/v0.3.0/specification/).

Card එක fetch කිරීමෙන් agent execution, tool use හෝ registration නොකරයි.
Default Demo mode එකේ task execution scriptedයි; Discover button එක user
ඉල්ලන වෙනම read-only network operation එකක්.

## 3. Authentication වෙන වෙනම configure කරන්න

Card endpoint එකට authentication අවශ්‍ය නම් **Card bearer token environment
variable** තුළ `ORCHESTRATION_AGENT_CARD_TOKEN` වැනි reference එකක් දාන්න.
Token value UI එකට paste නොකරන්න. App process එක start කරන terminal එකේ env
variable set කරලා තිබිය යුතුයි:

```powershell
$env:ORCHESTRATION_AGENT_CARD_TOKEN = 'your-card-access-token'
.\target\debug\orchestration-desktop.exe
```

ඒ token එක **user දුන් card URL එකට පමණක්** යවයි. Card එකේ execution URL එකට
credential copy නොකරයි. Agent editor එකේ execution bearer variable වෙනම
configure කරන්න. Server policy අනුව ඒ credentials එකම හෝ වෙනස් විය හැකියි.
Environment-variable token values checkpoint එකේ save කරන්නේ නැහැ.

Preview එක global සහ per-skill authentication requirement තිබේද කියලා පෙන්වනවා.
OAuth login, API-key header, mTLS හෝ authenticated extended-card flow මෙහි auto
configure නොකරයි. Execution connector එක optional HTTP bearer token පමණක් යවයි;
වෙනත් scheme එකක් අවශ්‍ය නම් service-specific integration අවශ්‍යයි.

## 4. Metadata සහ සැබෑ හැකියාව වෙන් කරලා බලන්න

Card එකේ skill එකක් තිබුණා කියලා ඒ skill එකේ quality app එක verify කළ බවක්
නොවේ. Card descriptions/tags **service claims** ලෙස escaped text පෙන්වනවා.
ඒවා agent instructions, local permissions හෝ context-sharing consent ලෙස
යොදන්නේ නැහැ. Remote instructions/permissions වැනි extra fields import නොකරයි.

**Capability-based automatic task assignment දැන් implement කරලා තියෙනවා.**
Use these details පසු valid lowercase skill IDs/tags editor එකට suggestions ලෙස
යොදනවා; user review/edit කරලා Save කළ capabilities පමණක් routing සඳහා භාවිතා
කරනවා. Card metadata එක තනිව permission හෝ routing authority එකක් නොවේ.
Manual agent selection එකත් භාවිතා කළ හැකියි. [Assignment guide](../README.md)
තුළ matching, priority, pinned selection සහ tests තියෙනවා.

## 5. Limits සහ failures

- A2A **0.3.0 JSON-RPC text subset** පමණයි. JSONRPC interface එකක් සහ අවම වශයෙන්
  `text/plain` input/output skill එකක් තිබිය යුතුයි. Omitted preferredTransport
  type-definition default අනුව JSONRPC ලෙස සලකනවා.
- HTTPS remote හෝ loopback HTTP; URL credentials/query/fragment තහනම්.
  Redirects සහ proxy routing disabled. Linked icons/docs/auth URLs fetch නොකරයි.
- Whole fetch deadline seconds **15**, response **256 KiB**, skills **32**,
  additional interfaces **8** සහ bounded descriptions/tags/modes.
- Missing fields, duplicate skill IDs, unsupported protocol/transport, oversized
  metadata සහ required unsupported extensions reject කරනවා. Raw HTTP error
  bodies සහ credential values error UI එකට යවන්නේ නැහැ.
- Signature verification නැහැ. Full A2A compliance, transport failover, registry
  catalogs සහ automatic card refresh මේ increment එකට අයත් නැහැ.
- URL edit කරන අතරතුර old fetch response එක ආවත් stale preview භාවිතා නොකරයි.
  Fetch error එකක් වුණොත් agent save නොවෙයි; existing agents වෙනස් නොවෙයි.

## 6. Code සහ verification

| File | වගකීම |
|---|---|
| [Discovery adapter](../crates/llm/src/discovery.rs) | Bounded GET, redacted errors, declared transport selection සහ card parsing |
| [Domain](../crates/domain/src/prototype.rs) | Reviewed metadata types/limits; old agents සඳහා optional default |
| [Service](../crates/api/src/desktop.rs) | Workspace lock නොතබා read-only fetch; normal validated SaveAgent transaction |
| [UI](../apps/desktop/src/discovery.ts) | Preview සහ safe agent defaults |
| [Browser test](../apps/desktop/tests/discovery.spec.ts) | Actual Rust bridge, stale/malformed preview, no auto registration, separate credentials සහ execution |
| [Native test](../apps/desktop/tests/native-discovery.mjs) | Actual Tauri fetch/review/save සහ independent authenticated Python service workflow |

```powershell
cargo test --workspace --locked -j 1
cargo build -p api --example ui_bridge --locked -j 1
cd apps/desktop
npm.cmd run test:ui
npm.cmd run tauri build -- --debug --no-bundle
npm.cmd run test:discovery-native
```

Original discovery increment එකේ Rust tests **72**, browser tests **4** සහ native
discovery flow pass වුණා. Assignment increment පසු latest counts **80 / 5**;
[verification record](VERIFICATION.md) සහ [native evidence](../README.md)
බලන්න. Tests synthetic service භාවිතා කරනවා; සැබෑ provider agent එකේ හැකියාවන්
සහ task quality වෙනම evaluate කළ යුතුයි.
