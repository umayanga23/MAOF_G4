# Contribution roadmap

Status: 2026-10-12. R3 and R4 are not implemented simply because the UI has a
timeline and controlled document tools.

| Track | Starting modules | Next contributor task | Acceptance criteria |
|---|---|---|---|
| R1 | Coordinator, Research lab | Matched workload evaluation harness | Identical inputs; separate waits; recorded seeds/versions; no unmeasured improvement claims |
| R2 | `crates/api/src/remote.rs`, external adapter | Fixtures for accepted-then-timeout, unsupported cancel and late completion | Reuse IDs; no blind resubmit; retain uncertainty; settle stale/duplicate results once |
| R3 | Domain events, coordinator, UI | Causal dispatch/result records and read-only inspector | Actual payload correlation, failure label and sequence shown; preserve original history |
| R4 | Tools, local adapter, domain jobs | Catalog and authorized keyword ranking | Stable ties; configurable top-k; required tools/fallback; schema size and recall metrics |
| R5 | Transport, memory, supervisor | Message-byte and actual-context counters | No secret/payload logging; distinguish control vs model traffic; reproducible exports |

## Implementation order

1. Preserve admission, durable dispatch and remote recovery contracts.
2. Add causal records/counters (R3/R5) and standalone catalog selection (R4).
3. Wire selected schemas to a bounded live model/tool loop, with permission rechecks
   and unknown-effect handling before enabling side-effecting tools.
4. Build R3 inspection, then fork/replay using isolated state and fresh approval scope.
5. Run policy, tool-selection and communication comparisons and publish evidence.

## Excluded scope

LMS/browser automation, Gmail/Calendar workflows, marketplaces, installer signing,
hosted multi-user services, arbitrary code execution and a broad assistant product.
Discovery, capability assignment, document policy, bounded memory and resource
guards remain support infrastructure, not separate novelty claims.
