# Testing and evidence

Use isolated data under `.cache`, never real account credentials or personal app history.
From the root:

```powershell
python doc/tools/check_project_docs.py
python scripts/check_publish.py
cargo fmt --all -- --check
cargo test --workspace --locked
cargo build -p api --example ui_bridge --locked
cargo run -p api --example smoke -- .cache/my-smoke-workspace
```

From `apps/desktop`:

```powershell
npm ci
npm run build
npm run test:ui
```

UI tests need Microsoft Edge and the actual Rust JSON-lines bridge. CI runs on
Windows. Native UI Automation scripts are optional and need a built executable,
WebView2 and an interactive Windows desktop.

| Check | Evidence |
|---|---|
| Rust tests | Coordinator, policy, recovery, permissions, transport and storage fixture behavior |
| Smoke | Coordinated Demo execution and saved results through the real service |
| Frontend build | Types and production assets compile |
| UI tests | Edit/launch/control/approval/export/discovery/assignment integration |
| Docs/publish checks | Local links and selected file/credential patterns |

None proves scientific novelty, live-model quality, remote exactly-once execution
or universal interoperability. The publish scanner is not comprehensive secret detection.

Research lab times are virtual; they are not observed model response times. Use
[research scope](RESEARCH_SCOPE.md) for controlled/live experiments. Executed
checks for this revision are in [verification](VERIFICATION.md).
