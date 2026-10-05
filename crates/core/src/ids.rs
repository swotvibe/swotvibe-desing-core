//! Permanent identity types and their generators.
//!
//! Ownership: identity types and how they are produced.
//! Not owned: node ordering or position. Identity must never encode sibling
//! order, and it must not change when a node is moved or renamed.
//!
//! Identity types are separate newtypes so a value of one kind can never be
//! passed where another is expected (see the technical specification, §5.3).
//! The concrete representation stays behind these project-owned types so that
//! no third-party type leaks into the public API.
//!
//! The default representation is UUID v4 (RFC 9562). Creation-time ordering is
//! not a document semantic, so the identifier is never used to encode sibling
//! order. Deterministic construction exists for tests and for importers that
//! provide their own identifiers.

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

/// The kind of document element an identity belongs to.
///
/// Used to report a type mismatch without leaking the underlying representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdKind {
    /// A design document.
    Document,
    /// A page that owns scene roots.
    Page,
    /// A scene node.
    Node,
    /// An asset referenced by nodes.
    Asset,
}

impl IdKind {
    /// The stable, human-readable name of this kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Page => "page",
            Self::Node => "node",
            Self::Asset => "asset",
        }
    }
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An error produced while constructing or validating an identity value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The identifier text was not a well-formed UUID.
    #[error("invalid {kind} id {value:?}: {source}")]
    Invalid {
        /// The kind of identity being parsed.
        kind: IdKind,
        /// The offending input text.
        value: String,
        /// The underlying parse failure.
        source: uuid::Error,
    },
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $kind:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            /// The kind of identity this type carries.
            pub const KIND: IdKind = $kind;

            /// Creates a fresh random identifier (UUID v4).
            ///
            /// Use [`Self::from_uuid`] or [`Self::deterministic`] when the
            /// caller must supply its own value.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an already-known UUID.
            ///
            /// This is the single entry point for importers that provide their
            /// own identifiers; callers must detect and resolve collisions
            /// before inserting the data (see §5.3).
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Derives an identifier deterministically from a namespace label
            /// and a caller-supplied value.
            ///
            /// The same inputs always produce the same identifier, which makes
            /// it suitable for reproducible tests and for importers that need a
            /// stable mapping. It must not be used to encode ordering.
            #[must_use]
            pub fn deterministic(namespace: &str, value: &str) -> Self {
                let mut hasher = Sha256::new();
                hasher.update($kind.as_str().as_bytes());
                hasher.update(&[0]);
                hasher.update(namespace.as_bytes());
                hasher.update(&[0]);
                hasher.update(value.as_bytes());
                let digest = hasher.finish();
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&digest[..16]);
                // Mark the version and variant bits so the value is a valid
                // UUID of the same shape as the random default.
                bytes[6] = (bytes[6] & 0x0f) | 0x40;
                bytes[8] = (bytes[8] & 0x3f) | 0x80;
                Self(Uuid::from_bytes(bytes))
            }

            /// Returns the underlying UUID.
            ///
            /// Kept for host boundaries such as file formats; it is not part of
            /// the identity contract and the project types remain the public API.
            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|source| IdError::Invalid {
                    kind: $kind,
                    value: s.to_owned(),
                    source,
                })
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdError;

            fn try_from(s: &str) -> Result<Self, Self::Error> {
                s.parse()
            }
        }
    };
}

define_id!(
    /// Identifies a design document.
    DocumentId,
    IdKind::Document
);
define_id!(
    /// Identifies a page within a document.
    PageId,
    IdKind::Page
);
define_id!(
    /// Identifies a scene node.
    NodeId,
    IdKind::Node
);
define_id!(
    /// Identifies an asset referenced by nodes.
    AssetId,
    IdKind::Asset
);

/// Computes the SHA-256 digest of `data`.
///
/// Exposed only to this crate's tests so the private hasher can be checked
/// against the official NIST vectors. Not public API.
#[doc(hidden)]
#[must_use]
pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finish()
}

/// A minimal SHA-256 implementation used only for deterministic identifier
/// derivation. It is private, dependency-free, and not used for security.
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    length: u64,
}

impl Sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    const fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffer_len: 0,
            length: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        if self.buffer_len > 0 {
            let take = (64 - self.buffer_len).min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffer_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.compress(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    fn finish(mut self) -> [u8; 32] {
        let bit_len = self.length.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            for byte in &mut self.buffer[self.buffer_len..] {
                *byte = 0;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buffer = [0; 64];
        } else {
            for byte in &mut self.buffer[self.buffer_len..56] {
                *byte = 0;
            }
        }
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);

        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, chunk: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let b = i * 4;
            *word = u32::from_be_bytes([chunk[b], chunk[b + 1], chunk[b + 2], chunk[b + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (&k, &wi) in Self::K.iter().zip(w.iter()) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k)
                .wrapping_add(wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}
