//! # swotvibe-layout
//!
//! The layout adapter contract.
//!
//! Ownership: translating product layout semantics (fixed/fill/hug) to and from
//! an external layout engine, and the layout result fingerprint.
//! Not owned: treating Taffy/Yoga as the document source of truth.
//!
//! An external layout engine does not own product semantics such as
//! fixed/fill/hug or text measurement. The adapter is defined and tested against
//! its contract before any library is committed behind it.
//!
//! ## Service contract
//!
//! - **Input:** an immutable snapshot of a subtree, container constraints,
//!   design units, layout settings, and a text/font measurement fingerprint.
//! - **Output:** bounds per `NodeId`, overflow/unsupported diagnostics, and a
//!   result fingerprint.
//! - **Dependency rule:** the adapter never returns a reverse reference to
//!   `DocumentEngine`; it never mutates the document.
//!
//! The concrete trait lands with the M0 slice. Engine selection (`DEC-LAYOUT`)
//! remains open.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
