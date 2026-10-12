# Contributing to MAOF_G4

Choose a task in one of the [five research tracks](doc/ROADMAP.md). Keep PRs focused
enough that a teammate can understand and verify the change.

## Branch workflow

```powershell
git clone https://github.com/umayanga23/MAOF_G4.git
cd MAOF_G4
git fetch origin
git switch dev-umayanga
git switch -c feature/r4-tool-selector
```

Use `feature/r1-*` through `feature/r5-*`, `fix/*`, or `docs/*`. Push your feature
branch and open a PR targeting **dev-umayanga**. Friends without write access can
fork the public repo and submit PRs. GitHub issue/PR templates record the track,
expected behavior, checks and unfinished work.

## Review requirements

1. Read the [architecture](doc/ARCHITECTURE.md) and relevant runtime guide.
2. Add meaningful tests for changed behavior and a relevant failure case.
3. Run the [checks](doc/TESTING.md) covering the change.
4. Update status/acceptance criteria when a roadmap task is implemented.
5. Keep synthetic fixtures, live observations and proposed benefits separate.

Do not commit `.cache`, `target`, `node_modules`, generated UI output, credentials,
OAuth files, app history, mail, LMS material or personal screenshots. Run
`python scripts/check_publish.py` and review the staged diff before publishing.
The scanner checks selected patterns/paths; it cannot detect every possible secret.

Shared-contract changes must preserve checkpoint defaults or document a migration.
Model/tool results cannot grant permissions. Preserve durable dispatch and unknown
usage handling. Replay must preserve the original run and create new identities;
tool relevance must not replace execution-time authorization.

## Research claims

Cite primary sources and versioned configurations. Do not claim all frameworks
broadcast messages, always inject every schema, or lack replay/recovery. Include
selection, summarization, routing and retry calls in total usage measurements.

The group should agree on a software license before describing this as licensed
open-source software. Public visibility alone does not grant a license.
