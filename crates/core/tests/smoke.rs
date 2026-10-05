//! Smoke tests for the core kernel scaffold.

use swotvibe_core::DocumentEngine;

#[test]
fn engine_can_be_constructed() {
    let engine = DocumentEngine::new();
    let _ = engine;
}
