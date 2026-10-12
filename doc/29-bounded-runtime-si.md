# Bounded runtime සහ worker failure handling

> Scope update (2026-10-12): the five-track [research scope](RESEARCH_SCOPE.md) and [roadmap](ROADMAP.md) supersede older full-product plans. This guide retains its original review/test dates.


Implementation increment: **2026-10-07**. Project එකේ agent setup, routing,
permissions සහ remote recovery පවත්වාගෙන communication සහ worker reliability
වැඩි කළා. මේ increment එක asynchronous storage writer එකෙන් වෙන් වූ වැඩක්.

## 1. දැන් එකතු කළ දේ

| කොටස | ක්‍රියා කරන හැටි |
|---|---|
| සාමාන්‍ය changes | Agent/settings/workflow changes සහ launches සඳහා queued commands 32 |
| Controls | Pause/resume/stop, report/document approvals සහ document revocation සඳහා වෙනම queued commands 8 |
| Fair handling | දැනට queue වූ normal command එකක් තිබුණොත් උපරිම controls 4කට පසු normal command එකක් process කරනවා; එක lane එක තුළ FIFO |
| Payload budget | Normal lane 2 MiB, control lane 64 KiB; එක් command එකකට 256 KiB / control එකකට 8 KiB |
| Overload | Queue හෝ byte budget full නම් `CommandQueueFull`; ලොකු request එකකට `CommandTooLarge`. එම request එක accept කරන්නේ නැහැ |
| Durable reply | Accepted change එක validate කරලා checkpoint save වුණාට පසුව success reply එක ලැබෙනවා |
| Worker results | Slots 8ක් ඇති bounded result channel. Dispatch intent save කරන්න කලින් result slot එක reserve කරනවා |
| Failure handling | Panic, result නොදී worker exit වීම සහ model observation timeout හඳුනාගෙන original attempt එක settle/escalate කරනවා |
| UI | Run screen එකේ **Engine health** expand කළාම queues, workers සහ refusal/failure counters බලන්න පුළුවන් |

Counts කියන්නේ queue වූ commands සඳහායි. මේවා encoded JSON byte accounting;
app එකේ සම්පූර්ණ memory හෝ IPC decoding memory සඳහා OS quota එකක් නොවේ.
Snapshot, export, model listing සහ discovery වැනි read paths mutation queue එකෙන්
යන්නේ නැහැ. Model report text characters 16,000 සහ error characters 512 දක්වා
කප්පාදු කරනවා; reported usage අතහැර දාන්නේ නැහැ.

Controls සඳහා reserve කළ queue space එක නිසා normal requests වැඩි වුණත් Stop
request එක submit කරන්න ඉඩ තියෙනවා. Control lane එකත් full වුණොත් explicit
refusal එකක් ලැබෙනවා. **දැනට process වන checkpoint/document transaction එක
ඉවර වෙනකම් control එකට බලා සිටින්න වෙනවා**; තවම guaranteed response latency
එකක් නැහැ. Queue fairness කියන්නේ command ordering; task scheduling policies
හතර වෙනස් කළේ නැහැ.

## 2. Worker crash වුණාම මොකද වෙන්නේ?

- Network call එකට පෙර worker fail වුණොත් known zero usage ලෙස reservation
  release කරලා task එක fail කරනවා. Worker fault එක blind retry කරන්නේ නැහැ.
- Remote call එක ගියා විය හැකි නම් usage uncertainty ලෙස රඳවාගන්නවා.
  Unknown usage කියන්නේ tokens zero කියන එක නොවේ.
- A2A saved task ID එකක් තිබුණොත් paused/pending remote state එක තබනවා.
  Resume කළාම same-ID status check කරනවා; අලුත් submission එකක් යවන්නේ නැහැ.
  Terminal response එකෙන් retained usage එක එක් වරක් reconcile කරනවා.
- Independent task branches ඉදිරියට යනවා. Failed task එක අනිවාර්ය dependency
  නම් පරණ dependency failure rules යෙදෙනවා.
- Stale/duplicate outcome එකකට original run/task/agent version/attempt fence
  නොගැළපුණොත් දෙවැනි settlement එකක් සිදු නොවේ.
- Panic payload එක run events/results වලට copy කරන්නේ නැහැ; safe error code
  එකක් පමණක් පෙන්වනවා. Development panic hook stderr වෙනම diagnostics වේ.

Model worker deadline: Demo delay + තත්පර 2; local call තත්පර 65;
external configured timeout + තත්පර 5. Adapter එකේම timeout එකත් තියෙනවා.
Local future timeout වීමෙන් remote server inference නතර වුණා කියලා තහවුරු
වෙන්නේ නැහැ. Blocking document read එකකට hard timeout/process kill එකක් නැහැ.
Document policy check, open/read සහ durable settlement එකම ordered transaction
gate එකේ කරනවා; later writer increment එකේ snapshot mutex එක I/O සඳහා
තබාගන්නේ නැහැ. Result channel එකට එන්නේ settled operation acknowledgement එක.

Service එකකට driver lifetime එකක් පමණයි. Competing driver එකක් dispatch
කරන්නේ නැහැ. Driver/runtime shutdown වුණොත් checkpoint නැවත open කිරීමෙන්
recover කරන්න; same Service එක තුළ driver replace කිරීම තව implement කරලා
නැහැ. **Engine stopped** නම් app එක reopen කරන්න. Abrupt driver shutdown එකේ
session worker counters stale විය හැකියි; checkpoint recovery තමයි authority.

## 3. ඔබ test කරන්නේ කොහොමද?

1. Updated `target/debug/orchestration-desktop.exe` open කරන්න.
2. Settings තුළ Demo mode, workers 2–8 සහ සුදුසු memory/budgets තෝරන්න.
3. Independent tasks ඇති workflow එකක් start කරන්න. Run screen එකේ **Engine
   health** expand කරන්න; workers සහ pending counts බලන්න.
4. **Pause → Resume → Stop run** කරන්න. Stop පසු new steps start නොවී
   in-flight results settle විය යුතුයි. Cancelled run එකේ reservations clear
   විය යුතුයි; live calls වල unknown usage නම් තව retain විය හැකියි.
5. Document/report Ask flows සහ external A2A resume/status flowsත් කලින්
   [testing guide](TESTING.md) හා
   [remote recovery guide](25-external-task-recovery-si.md) අනුව test කරන්න.

Queue overload සහ synthetic crash tests automation එකෙන් repeat කරන්න:

```powershell
# Repository root
cargo test --workspace --locked -j 1
cargo build -p api --example ui_bridge --locked -j 1
cd apps/desktop
npm.cmd run test:ui
# Updated native executable අවශ්‍යයි; own hidden test workspace එකක් භාවිතා කරයි
npm.cmd run test:runtime-native
npm.cmd run test:remote-native
npm.cmd run test:assignment-native
```

Rust tests වල panic/missing-result/deadline injection `cfg(test)` පමණයි.
Production app එකේ hidden crash-trigger setting එකක් නැහැ. Native runtime
test එක normal changes 128ක් එකවර යවලා reserved Stop එක සහ eight-worker
draining පරීක්ෂා කරනවා. Evidence හා actual counts
[verification record](VERIFICATION.md) තුළයි. මේවා Demo/HTTP
fixtures; live Ollama quality හෝ measured latency improvement evidence නොවේ.

Reply නැති වුණා කියලා accepted command එක cancel වෙන්නේ නැහැ. Workspace
state බලලා retry කරන්න; general command idempotency key තව නැහැ. Refusal
message එක explicit **no command accepted** නම් එම request එක queue වී නැහැ.
Health counters මේ app session එකට පමණයි; restart වෙද්දී reset වෙනවා.
Accepted `worker_failed` event එක durable run history තුළ තියෙනවා.

## 4. ඉතිරි foundation වැඩ

Later update: [asynchronous checkpoint writer](30-async-durable-writer-si.md)
දැන් implement කළා. Snapshot mutex එක disk I/O සඳහා තබාගන්නේ නැහැ; save/publish
පිළිවෙළ සඳහා transaction gate එක තව retain කරනවා. Full journal, slow effect
preemption සහ remaining process-crash boundaries තව work.
ඉන්පසු agreed roadmap එකේ **MCP/tool registry**, conditional review loops,
streaming, visual editor සහ long-term retrieval දිගටම යනවා.

Implementation files:
[mutation transport](../crates/api/src/transport.rs),
[supervised driver](../crates/api/src/supervision.rs),
[coordinator](../crates/orchestrator/src/prototype/mod.rs),
[transport tests](../crates/api/src/transport_tests.rs),
[native overload test](../apps/desktop/tests/native-runtime.mjs).

Reference patterns: [Tokio bounded channels](https://tokio.rs/tokio/tutorial/channels)
සහ [JoinSet tracking](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html).
Project implementation එක මේ patterns මත explicit durable admission සහ usage
settlement යොදනවා; library එක තනිවම remote exactly-once execution දෙන්නේ නැහැ.
