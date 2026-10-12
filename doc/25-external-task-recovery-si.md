# External task recovery සහ cancellation

> Scope update (2026-10-12): the five-track [research scope](RESEARCH_SCOPE.md) and [roadmap](ROADMAP.md) supersede older full-product plans. This guide retains its original review/test dates.


2026-10-07 increment එකේ A2A agent එකකට යැවූ task එකේ identity save කරනවා.
App එක restart වුණාම **එම remote task එකේ status/result බලනවා**. Task එක අලුතින්
නැවත යවන්නේ නැහැ. මේ guide එක [external-agent setup](24-external-agents-si.md)
මත පදනම් වෙනවා; features අටේ ඉදිරි වැඩ [roadmap](ROADMAP.md)
තුළ තියෙනවා.

## 1. දැන් වැඩ කරන දේ

| Feature | App එක කරන දේ |
|---|---|
| Submission record | Network requestට පෙර submission identity සහ intent checkpoint කරනවා |
| Remote task identity | A2A response එකෙන් task ID/state ලැබුණු විට ඊළඟ poll එකට පෙර save කරනවා |
| Restart recovery | Interrupted run එක recover/resume කළ විට saved ID සඳහා `tasks/get`; අලුත් `message/send` නොවේ |
| Stop | Pending task ID තිබෙන විට best-effort `tasks/cancel`; queued request එක තවම යවා නැත්නම් යවන්නේ නැහැ |
| Unconfirmed cancellation | Remote work තවම තිබිය හැකි බව පෙන්වනවා; local Cancelled run එකේ **Check remote status** මගින් පරීක්ෂා කරන්න පුළුවන් |
| Usage settlement | එක step එකේ retained uncertainty පසුව ලැබෙන usage සමඟ එක්වරක් settle කරනවා; stale local result reject කරනවා |
| Reviewable history | Run screen එකේ External task status, cancellation සහ explanation; JSON exports තුළ remote fields/events |

Remote status checks සඳහා අලුත් **agent-call budget** charge එකක් හෝ token
reservation එකක් නැහැ. නමුත් `tasks/get`/`tasks/cancel` සැබෑ HTTP requestsයි.
Worker slot, agent serialization සහ declared RAM admission තවම අදාළයි. Server
usage නොදෙන්නේ නම් app එක token count invent කරන්නේ නැහැ; uncertainty රැඳෙනවා.

## 2. මුලින් සරල restart test එක කරන්න

Ollama හෝ paid account එකක් නැතිව independent sample service එකෙන් test කරන්න:

```powershell
cd F:\multi-agent-orchestrator
python examples/external_agent_server.py --port 8787 --delay 20
```

මේ terminal එක running තබාගන්න. Sample server එක tasks memory එකේ තබන නිසා
**desktop app එක පමණක්** close/reopen කරන්න. Server එක restart කළොත් sample
task IDs නැති වෙනවා. Real agent service එක durable task storage දිය යුතුයි.

1. Desktop app එක open කරන්න: `target/debug/orchestration-desktop.exe`.
2. Agents → Add agent. නම `Recovery Demo`; Agent connection එක
   **External A2A JSON-RPC**; endpoint `http://127.0.0.1:8787/a2a`; timeout 60 seconds.
   Context sharing මේ test එකට අවශ්‍ය නැහැ. Save කරන්න.
3. Workflows → එක task එකක් ඇති template එකක් හදන්න. ඉහත agent එක තෝරන්න;
   steps **1**, dependencies/document නැහැ; Save report off කරන්න.
4. Settings → **Live — local models and external agents** mode. Overview එකෙන් ඒ workflow එක
   තෝරලා `Summarize a sample research task` prompt එකෙන් Start workflow කරන්න.
5. Run screen එකේ **External task status** යටතේ remote task ID ලැබෙනකල් බලන්න.
   Task එක තවම working වෙද්දී app එක close කරන්න. Sample server එක running තබන්න.
6. App එක නැවත open කරලා Run history එකෙන් run එක open කරන්න.
   **Recover checkpoint → Resume** කරන්න. එම remote ID එකේ result ලැබී run එක
   Completed විය යුතුයි. Export JSON කරලා ID, one completed step සහ agent calls
   **1** බව බලන්න. Sample service එක model call නොකරන නිසා usage **0**යි.

Default app data directory එක වෙනස් නොකරන්න. Custom `ORCHESTRATION_LAB_DATA_DIR`
භාවිතා කරනවා නම් දෙවරම එකම directory එක දිය යුතුයි. Task එක 20 secondsට පෙර
අවසන් වුණොත් pending run එකක් නැති නිසා restart test එක නැවත කරන්න.

## 3. Stop සහ status checks

නැවත delayed task එකක් start කරලා remote ID ලැබුණු පසු **Stop run** කරන්න.
Sample server එකේ pending task එක canceled විය යුතුයි. Local run එක Cancelled,
remote state `canceled`, cancellation `confirmed` ලෙස පෙන්විය යුතුයි.
Stopped task එකේ result local output/progress ලෙස accept කරන්නේ නැහැ; reported
usage තිබුණොත් accounting එකේ record කරනවා.

| Cancellation value | තේරුම |
|---|---|
| `none` | Cancellation ඉල්ලලා නැහැ |
| `requested` | Local stop accepted; remote request තවම යවා නොතිබිය හැකියි |
| `sent` | Durable cancellation intent තිබෙනවා; response තවම confirm නැහැ |
| `confirmed` | Server task state `canceled`; හෝ request යැවීමට පෙර stop වූ බව app එක දන්නවා |
| `unconfirmed` | Server cancel ප්‍රතික්ෂේප කළා, response නැහැ, timeout වුණා හෝ task එක cancel නොවී අවසන් වුණා |

Cancellation **confirmed වුණත් කලින් සිදු වූ email/file/trade වැනි effects undo
වුණා කියලා අදහස් කරන්නේ නැහැ**. A2A cancellation server support/state මත
පදනම් වන best-effort operation එකක්. [A2A v0.3 specification](https://a2a-protocol.org/v0.3.0/specification/).

Stop පසු status polling timeout වුණොත් local run එක Cancelled විය හැකි අතර
remote step එක pending ලෙස පවතී. **Check remote status** click කළාම saved task
එක පමණක් query කරනවා. කලින් cancel intent යවා තිබෙනවා නම් blind cancel retry
නොකරයි. No new submission; no new local workflow work. Pending remote identity
තියෙන run එක export කරන්න පුළුවන්; archive කරලා tracking record මකා දැමීම
block කරනවා. Server එක unavailable නම් reason බලලා පසුව නැවත status check කරන්න.

App restart වුණේ Stop request පසු නම් **Recover checkpoint** කලින් stop intent
රැකගෙන cancellation/status observation ඉදිරියට ගෙනයනවා. Normal interrupted
run එකක් recover කළාම paused වෙනවා; එයට Resume අවශ්‍යයි.

## 4. Task ID ලැබෙන්න කලින් connection එක කැඩුණොත්

Server එක request accept කරලා response එක නැති වෙන්න පුළුවන්. එවිට app එකට
remote task ID නැහැ; `submitting`/`unknown` record එක රැඳෙනවා. මේ task එක
automatically නැවත submit කරන්නේ නැහැ. Remote service logs/dashboard එකෙන්
original submission identity පරීක්ෂා කරන්න. App එක remote work නතර කළ බව
හෝ exactly-once external execution බව guarantee කරන්නේ නැහැ.

Submission state `queued` ලෙස durableව තිබුණේ request gateට පෙර නම් app එක
තවම send නොකළ බව දන්නවා. Restart/resume ඒ queued request එක submit කළ හැකියි;
Stop කළොත් `not_submitted` ලෙස settle කරනවා. Network-send intent පසු outcome
unknown වූ අවස්ථාව එයින් වෙනස්.

## 5. Scope සහ implementation

- මේ lifecycle support එක **A2A v0.3 JSON-RPC text/task polling subset** සඳහායි.
  Simple HTTP JSON connector එකට standardized task status/cancel contract නැහැ;
  ඒ connector එකේ recovery semantics වෙනස් කරලා නැහැ.
- Endpoint, agent version සහ auth variable reference run snapshot එකේ pin වෙනවා.
  Bearer token value checkpoint එකේ save නොකරයි; reopen කරන process එකට ඒ env
  variable එක තිබිය යුතුයි.
- Per-operation timeout සහ 120-poll cap තියෙනවා. Errors/timeout පසු normal run
  pause කරලා identity/uncertainty රඳවනවා; Resume saved status පරීක්ෂා කරනවා.
- `input-required`/`auth-required` states pause වෙනවා; interactive continuation
  තවම implement කරලා නැහැ. Agent Card discovery, SSE/gRPC සහ full A2A
  conformance මේ increment එකට අයත් නැහැ.
- Immediate Message response එකට durable remote task ID නැති නිසා result ලැබීම
  සහ local completion commit අතර crash වුණොත් manual review අවශ්‍ය විය හැකියි.
- Later increment එකේ [async checkpoint writer](30-async-durable-writer-si.md)
  implement කළා; startup recovery save synchronous වේ. Full journal, disk power-loss guarantees සහ
  general provider billing reconciliation තවම gap ලෙස තබාගෙන තියෙනවා.

| File | වගකීම |
|---|---|
| [Domain](../crates/domain/src/prototype.rs) | Optional remote record; old checkpoints සඳහා default |
| [Remote coordinator](../crates/orchestrator/src/prototype/remote.rs) | Suspend, uncertainty retention සහ checkpoint validation |
| [Step coordinator](../crates/orchestrator/src/prototype/mod.rs) | Reattach admission, stop/recover controls සහ once-only local settlement |
| [A2A client](../crates/llm/src/external.rs) | Bounded authenticated send/get/cancel RPCs සහ response identity checks |
| [Lifecycle service](../crates/api/src/remote.rs) | Persist-before-send/cancel, checkpointed observations සහ stale attempt fences |
| [Native test](../apps/desktop/tests/native-remote.mjs) | Actual desktop restart, cancel and manual reconciliation scenarios |

## 6. Reproducible verification

```powershell
cargo test --workspace --locked -j 1
cargo build -p api --examples --locked -j 1
cd apps/desktop
npm.cmd run test:ui
npm.cmd run tauri build -- --debug --no-bundle
npm.cmd run test:remote-native
npm.cmd run test:external-native
npm.cmd run test:native
```

Rust tests independent service fixtures මගින් saved-ID restart, lost submission
response, no replay, known/uncertain usage, cancel reject/lost response, stop before
send සහ stale completion verify කරනවා. Native test එක actual Tauri/WebView2
process එක තමන්ගේ isolated workspace සමඟ reopen කරනවා. Test එක close කරන්නේ
එය start කළ app process එක පමණයි; personal app history භාවිතා නොකරයි.

Observed results සහ evidence සඳහා [verification record](VERIFICATION.md)
බලන්න. Synthetic service checks actual cloud-agent quality හෝ cross-framework
benchmark එකක් ලෙස භාවිතා කරන්න එපා.
