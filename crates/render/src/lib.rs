//! # swotvibe-render
//!
//! The render adapter contract.
//!
//! Ownership: scene extraction, invalidation, update scheduling, cache
//! management, and handling background differences.
//! Not owned: building a full rasterizer (that needs a separate justified
//! decision), and never writing back to the document.
//!
//! ## Service contract
//!
//! - **Input:** a scene snapshot, layout results, resolved assets, a scale/color
//!   configuration, and the backend identity and version.
//! - **Output:** a surface or draw commands, diagnostics, and resource info.
//! - **Dependency rule:** there is no reverse dependency from the model to the
//!   renderer. A backend/font/color difference is part of the reference-image
//!   fingerprint.
//!
//! The backend choice (`DEC-RENDERER`: Skia vs Vello CPU vs Vello GPU/wgpu) is
//! still open and is not fixed in this scaffold.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
