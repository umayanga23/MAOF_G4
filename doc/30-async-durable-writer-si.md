# Asynchronous durable checkpoint writer

> Scope update (2026-10-12): the five-track [research scope](RESEARCH_SCOPE.md) and [roadmap](ROADMAP.md) supersede older full-product plans. This guide retains its original review/test dates.


Implementation increment: **2026-10-07**. Checkpoint සහ exports සඳහා වෙනම
bounded writer එකක් දැන් තියෙනවා. Disk write අතරතුර UI එකට last committed
snapshot එක ලබාගන්න පුළුවන්; save නොවූ changes success ලෙස පෙන්වන්නේ නැහැ.

## 1. ක්‍රියා කරන හැටි

```mermaid
sequenceDiagram
    participant C as Command / worker
    participant G as Transaction gate
    participant W as Async writer
    participant D as Disk
    participant S as Published state
    C->>G: Acquire ordered transaction
    C->>C: Validate and prepare next state
    C->>W: Transfer next state and owned gate
    W->>D: Encode, write, sync, replace checkpoint
    Note over S: UI reads last committed state during disk I/O
    D-->>W: Save success or failure
    alt Save succeeded
        W->>S: Publish committed state
        W-->>G: Release gate
        W-->>C: Durable acknowledgement
        C->>C: Dispatch permitted work / reply success
    else Save failed
        W->>S: Keep old state; record storage fault
        W-->>G: Release gate
        W-->>C: Error; new dispatch disabled
    end
```

Transaction gate එක mutations පිළිවෙළට කරනවා. Snapshot mutex එක state copy
සහ final publication සඳහා පමණයි; prepared copy එක validate කරන්නේ gate එක යටතේ.
JSON encoding, file writes,
sync සහ replacement **snapshot mutex එකෙන් පිටත blocking worker එකක** කරනවා.
Writer එක වරකට එක් disk operation එකක් කරයි; parallel checkpoint replacements
නිසා පරණ state එකෙන් අලුත් state overwrite වීමට ඉඩ දෙන්නේ නැහැ.

| Boundary | දැන් තියෙන හැටි |
|---|---|
| Checkpoint queue | Pending slot 1; transaction gate නිසා එක් checkpoint transaction එකක් පමණක් prepare/write/publish වෙයි |
| Export queue | Pending slots 2; checkpoint queue එකෙන් වෙනම space |
| Export preparation budget | Preparing/queued/writing exports එකතුව උපරිම 3; cancelled caller එකක් accepted work එකේ permit release නොකරයි |
| Writer ordering | Queued export එකක් තිබුණොත් උපරිම checkpoints 4කට පසු export එකක්; දැනට ක්‍රියා කරන operation එක preempt නොකරයි |
| Checkpoint byte bound | Save සහ load දෙකටම 100 MiB; oversized JSON encoding old checkpoint/temp file එක touch කරන්න පෙර fail වෙයි |
| Success reply | Disk save සහ state publication දෙකම අවසන් වූ පසුව පමණයි |
| Model dispatch | Saved dispatch intent පසුව පමණක් worker start; final effect gate Stop/fault/fence නැවත check කරයි |
| Failure | Old published state retain කර storage fault දමා new dispatch freeze කරයි |

මේ queue counts/bounded encoding process එකේ සම්පූර්ණ RAM සඳහා OS quota එකක්
නොවේ. Current run/document/context limitsත් තවම යෙදෙනවා.

Accepted commit එකේ caller/worker future cancel කළත් writer එක **owned gate**
එක රඳවාගෙන save සහ publish කරනවා. එතෙක් ඊළඟ mutation එක old state එකෙන්
prepare වෙන්නේ නැහැ. Accepted export එකත් caller නැති වුණා කියලා අතුරුදහන්
කරන්නේ නැහැ. Reply නැති වුණොත් state/export directory බලලා retry කරන්න;
general command idempotency key තව නැහැ.

## 2. Stop, permissions සහ remote calls

Stop/approval requests සඳහා කලින් එකතු කළ reserved control lane එක දිගටම
තියෙනවා. Slow save එකකදී UI snapshots සහ queued-control counters බලන්න පුළුවන්.
**Control success reply එකට දැනට ක්‍රියා කරන transaction එක අවසන් වෙන්න
බලා සිටින්න වෙනවා.** මේ increment එක immediate Stop ACK හෝ fixed latency
guarantee එකක් දෙන්නේ නැහැ; අසාර්ථක save එකක් සාර්ථක control එකක් ලෙසත්
පෙන්වන්නේ නැහැ.

Model workersගේ final gate එකත් transaction ordering එකට යටත්යි. A2A
submission/status/cancel markers writer එකෙන් save කරලා publish කළ පසුව තමයි
ඊළඟ protocol operation කරන්නේ. Caller timeout වුණත් accepted marker write
එක continue වෙයි; saved identity/uncertain usage conservatively retain කරනවා.

Document read final permission check, file open/read සහ settlement එකම
transaction gate එකේ තියෙනවා. Snapshot mutex එක file read/sync සඳහා තබාගන්නේ
නැහැ. Revoke commit එක gate එකට පෙර නම් read එක deny වෙයි. Read එක gate එක
ලැබීමෙන් පසුව revoke/Stop එකට එය අවසන් වන තෙක් wait කරන්න වෙනවා; කලින්
කළ read එක undo කළ නොහැකියි. Hard document I/O cancellation තව නැහැ.

Imports blocking worker එකෙන් private copy එක ලියා පසුව registration commit
කරයි. Registration save fail වුණොත් orphan copy ඉතිරි විය හැකියි; එය granted
document එකක් ලෙස නොසලකයි. Archive command එක run export සාර්ථක වූ පසුව
checkpoint එකෙන් run එක ඉවත් කරනවා. Export file සහ checkpoint එක එක්
cross-file atomic transaction එකක් නොවේ; checkpoint failure පසු old run retain
වෙන අතර export copy එක තිබිය හැකියි.

Research experiment export calculation writer එකෙන් පිටත bounded blocking
preparation එකක කරනවා. එයින් calculation එක checkpoint writer occupy කරන්නේ
නැහැ. JSON/CSV exports writer එකෙන් පිළිවෙළට ලියනවා. Export queue full නම්
`WriterQueueFull` error එකෙන් එම export එක accept නොකළ බව පෙන්වනවා.

## 3. App එකෙන් බලන හැටි

1. පරණ app window එක close කර updated `target/debug/orchestration-desktop.exe`
   නැවත open කරන්න. Existing workspace format එක වෙනස් කරලා නැහැ.
2. Workflow එකක් run කර **Run history → Engine health** expand කරන්න.
3. **Checkpoint writer**, queued checkpoints/exports, exports in flight,
   checkpoints saved, failures සහ last checkpoint duration බලන්න.
4. Agent/settings edit කර save කරන්න. Success message පසු app restart කළාම
   ඒ change එක තිබිය යුතුයි. Pending save එකේ UI පෙන්වන්නේ කලින් committed state.
5. කලින් [testing guide](TESTING.md) අනුව Pause/Resume/Stop,
   document/report approvals, revoke සහ export/archive flows test කරන්න.
6. External A2A workflow සඳහා [recovery guide](25-external-task-recovery-si.md)
   අනුව saved-ID resume/cancel/status tests කරන්න.

Writer counters process session එකට පමණයි; restart වෙද්දී reset වෙනවා. Last
checkpoint duration එක observation එකක්; benchmark හෝ maximum delay එකක් නොවේ.
එයට encoding/write/publication time ඇතුළත්; command queue wait හෝ pure disk latency නොවේ.
Writer idle කියන්නේ engine work අවසන් කියන එක නොවේ.

Automated checks:

```powershell
# Repository root
cargo test --workspace --locked -j 1
cargo build -p api --example ui_bridge --locked -j 1
cd apps/desktop
npm.cmd run test:ui
# Updated executable; isolated hidden workspaces, personal data වෙනස් නොකරයි
npm.cmd run test:runtime-native
npm.cmd run test:remote-native
npm.cmd run test:assignment-native
```

Slow write සහ writer panic simulation Rust test build එකට පමණයි. Production
app එකේ fake disk-delay/crash setting එකක් නැහැ. Actual results/counts
[verification record](VERIFICATION.md) තුළ තියෙනවා.

## 4. මේ increment එකේ සීමාව සහ ඊළඟ වැඩ

මෙය **asynchronous checkpoint writer** implementation එකක්. Append-only
execution journal, command deduplication, database migration chain, full
process-crash/commit matrix හෝ power-loss durability claim එකක් තව නැහැ.
Startup load/validation/recovery save synchronous වේ; running service එකේ
checkpoint/export writes තමයි මේ increment එකෙන් background එකට ගියේ.

Commit එක in flight නම් same-process Service drop කිරීමෙන් disk task එක kill
වෙන්නේ නැහැ; workspace lock එක operation අවසන් වන තෙක් තබාගනී. Runtime/process
shutdown එකේ incomplete writes restart recovery boundary එකට යටත්යි. Local
observer task abort කිරීම process crash එකක් ලෙස සලකන්න එපා.

ඊළඟ agreed feature **MCP/tool registry**: මුලින් එක් read-only tool integration,
typed arguments, scoped permissions, bounded results සහ final effect gate.
Full journal/effect-state refactor එක separately roadmap එකේ පවතී.

Source:
[writer](../crates/api/src/durable.rs),
[transaction integration](../crates/api/src/desktop.rs),
[worker dispatch/settlement](../crates/api/src/supervision.rs),
[A2A markers](../crates/api/src/remote.rs),
[writer tests](../crates/api/src/durable_tests.rs),
[checkpoint storage](../crates/storage/src/checkpoint.rs).
