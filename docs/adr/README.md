# Architecture Decision Records

Each significant technical decision gets one ADR here, using the template in
[`template.md`](./template.md). Keep an ADR focused on a single decision and
link it from the relevant section of the technical specification.

Decision status is explicit in each ADR: accepted records capture project
choices; proposed records describe a recommendation awaiting its stated
acceptance gate. Open decisions tracked in the specifications (see `DEC-*` and
`CORE-*` IDs) move here when a recommendation or evaluation plan needs a durable
record.

## Records

| ADR | Decision | Status |
|---|---|---|
| [0001](./0001-core-model-invariants.md) | M0 model, identity, tree order, and storage default | Accepted |
| [0002](./0002-coordinate-contract.md) | Coordinate and transform contract | Accepted; fixtures committed |
| [0003](./0003-file-container.md) | Native ZIP64 document bundle profile | Codec and CLI implemented; release evidence remains open |
| [0004](./0004-undo-budget.md) | Session undo retention policy | Accepted policy; implement estimator and benchmark before numeric byte default |
| [0005](./0005-backend-evaluation.md) | Bounded layout, text, renderer, and Boolean evaluations | Accepted evaluation plan; backends remain open |
| [0006](./0006-text-backend-parley.md) | Text backend for the M0 slice: Parley | Temporary; `DEC-TEXT-AR` open |
| [0007](./0007-layout-backend-taffy.md) | Layout backend for the M0 slice: Taffy | Temporary; `DEC-LAYOUT` open |
| [0008](./0008-render-backend-vello-cpu.md) | Render backend for the M0 slice: vello_cpu | Temporary; `DEC-RENDERER` open |
