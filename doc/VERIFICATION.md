# Scoped repository verification

Revision: **2026-10-12**, Asia/Colombo. Executed on Windows with Rust 1.96.1,
Node 22.22.2 and Python 3.12.10. Cargo/npm lockfiles describe dependencies.

| Command/check | Result |
|---|---|
| `cargo check --workspace --offline` | PASS, including the Tauri shell |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace --locked` | PASS: 82 tests; 0 failures |
| `cargo build -p api --example ui_bridge --locked` | PASS |
| `npm run build` | PASS: TypeScript checks and Vite production build |
| `npm run test:ui` | PASS: 5 Microsoft Edge/Rust service integration tests |
| Demo smoke example | PASS: 3 tasks, 9 scripted attempts, 288 scripted tokens, 1 artifact; saved run/report survived reopening |
| Documentation checker | PASS: local links in 18 Markdown files |
| Publication checker | PASS: 111 Git-visible files; working and staged text checked for selected sensitive patterns |
| `git diff --cached --check` | PASS |

Rust test breakdown: API 26, model/discovery adapters 15, coordinator 34, storage 3,
and tools/policy 4. The removed unused whole-run engine's tests are outside this
scoped revision. Logical-resource guard tests remain as supporting coverage.

UI tests cover workspace editing, dependent workflows, pause/resume, approvals,
exports, external HTTP/A2A integration, discovery and capability routing. They
use isolated synthetic data, not personal accounts. The tests bridge to the real
Rust service; native WebView UI Automation was not rerun in this cleanup.

The frontend initially hit a sandbox process-spawn restriction and passed when
rerun with process permissions. GitHub Actions CI has been configured; this local
record does not claim a hosted CI result.

No live-model quality, cross-framework performance, tool-selection savings,
visual replay behavior or scientific novelty is established by these checks.
R3/R4 remain planned as documented in the roadmap.

## Scope cleanup

Removed the unused whole-run engine and its domain contracts, account-specific
lecturer/LMS helpers, obsolete full-system docs/diagrams and diagram generators
from the active repository. Removed the dedicated deadlock-demo UI controls;
kept internal guards for existing checkpoint/test compatibility. Local recovery
copies are under `.cache/scope-cleanup-2026-10-12`, excluded from Git.
