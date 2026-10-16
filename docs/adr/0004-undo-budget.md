# ADR-0004: Session undo retention policy

- **Status:** configurable estimator and retention implemented (2026-10-16); numeric byte default and representative memory measurements remain open
- **Date:** 2026-10-05
- **Owner:** project
- **Spec IDs:** CORE-UNDO-01

## Context

History is session-scoped and stores inverse commands in `done` and `undone`
stacks. The original M0 capped the number of committed steps at 256 by default, but
one inverse can own a large deleted subtree. Step count alone therefore does
not represent retained memory. The repository does not yet have a representative
history corpus or product memory budget, so selecting a numeric byte default
would be arbitrary.

## Decision

Keep history out of the saved document. Retention will use two configurable
limits:

1. A maximum retained step count, keeping the current default of 256 until
   workload evidence justifies a change.
2. A maximum estimated retained byte count, covering both undoable and redoable
   entries. The estimate counts the owned allocations of inverse commands,
   including strings and vectors, and the recursively retained node data in
   subtree restoration commands. It is a retention estimate, not a hard process
   RSS guarantee; allocator metadata, shared allocations, and temporary working
   copies are outside its scope.

The byte limit is caller-configurable. No numeric default is published until
the benchmark corpus and supported host memory targets exist. The implemented
policy is:

- After commit, clear redo history, append the new inverse, then evict the
  oldest retained entries until both limits are met.
- Apply the step limit to the total retained timeline (`done + undone`), not
  just currently undoable entries. Undo/redo recomputes the charge of the
  replacement inverse and enforces both limits again. In this inverse-command
  representation a rename from a short string to a long string retains different
  data in each direction; preserving the old charge would undercount redo.
  Consequently replay can also evict distant history. The document replay still
  succeeds, and status reports the retained result.
- Never split a transaction's inverse. If a single newest inverse exceeds the
  entire byte budget, commit the document change but do not retain that inverse;
  expose this loss of undo through history status so the host can communicate
  it. This is preferable to retaining an entry that violates the stated cap.
- Preserve entry boundaries when evicting. History eviction never changes the
  document.
- Expose retained estimated bytes, undo/redo depths, configured limits, and
  whether the most recent commit was retained, so the host can report the
  actual undo capability.

## Options considered

- **Step count only:** simple and already implemented, but gives no bound on
  memory when an entry stores a large subtree.
- **Fixed byte number now:** appears concrete but would be an unsupported
  product guess without target memory budgets and representative files.
- **Serialize inverses to count bytes:** currently commands are runtime types,
  not a stable persisted format; serialized length would also omit container
  allocations. Keep the estimate internal to core rather than create a history
  serialization contract.
- **Persist history:** deferred; it expands file format, recovery, and
  compatibility requirements without an established need.

## Evidence and limits

Current implementation evidence: [`history.rs`](../../crates/core/src/history.rs)
implements both-stack limits and status. [`history_size.rs`](../../crates/core/src/history_size.rs)
counts retained entry values, command-vector capacities, boxed properties,
restored nodes/pages, strings and nested extension JSON. JSON map storage uses
an explicit per-entry estimate, since its allocator internals are not observable.
Unused top-level stack slots and transient working copies are outside the estimate.
[`history_integrity.rs`](../../crates/core/tests/history_integrity.rs) exercises
zero budgets, redo-only retention, inverse-size changes, metadata and oversized deletions.
There is no official standard that sets an appropriate undo-memory budget for a
design editor. This remains a workload and host-memory decision, not a fact
that library documentation can settle.

## Implementation gates

1. Add checked/saturating size accounting for every inverse command variant and
   every retained node field; update it when merging, undoing, redoing, clearing,
   and evicting entries.
2. Define behavior for zero-byte and one-entry-over-budget cases exactly as
   above; keep the document transaction outcome independent from history
   retention.
3. Add public status accessors without exposing internal vectors or changing
   the saved document schema.
4. Build representative create, rename, reorder, subtree delete, restore, and
   coalesced-edit workloads. Measure estimated versus allocator-observed memory
   on supported targets before setting a default byte budget.
5. Verify eviction order, both-stack accounting, redo clearing, and oversized
   entries before treating memory retention as implemented.

## Risks

The estimate will not equal resident memory. The host must treat it as a
consistent admission/retention policy, not a process-wide memory controller.
The oversized-entry behavior means an accepted edit may have no undo record;
status must be surfaced at the same commit boundary. Custom `commit_with` calls
still clone the pre-batch document; the engine's ordinary commit and replay paths
use self-contained removal effects. This ADR does not claim to bound
temporary transaction memory; that cost needs separate profiling.

## Follow-up

Accounting and status are implemented in the core through `HistoryStatus`.
Application and host integration are maintained separately from this core-only change.
Keep 256 total retained steps as the only default; the byte ceiling remains
unconfigured. Benchmark representative workloads and allocator-observed memory
before selecting a product default or claiming a process-memory bound.
