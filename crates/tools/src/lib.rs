//! # swotvibe-tools
//!
//! Headless tooling built on the core kernel: import, conversion, and evaluation
//! flows that do not need a user interface.
//!
//! These tools depend on the kernel; the kernel never depends on them. Bundle
//! filesystem operations and the CLI live in the binary adapter; unsafe Win32
//! replacement code is isolated there rather than in this reusable library.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use swotvibe_core::DocumentEngine;

/// Returns the crate name, used by smoke tests and the CLI banner.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Builds a fresh engine through the public kernel entry point.
///
/// This exists to prove the dependency direction (tools -> core) compiles and
/// links independently of filesystem operations.
#[must_use]
pub fn new_engine() -> DocumentEngine {
    DocumentEngine::new()
}
