//! Verifies the private SHA-256 used for deterministic ids against the
//! official NIST / FIPS 180-4 example vectors.
//!
//! These are the published test vectors, so a correct implementation must
//! reproduce the exact digests:
//!
//! - `"abc"` -> `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`
//! - `"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"`
//!   -> `248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1`

use swotvibe_core::ids::sha256_digest;

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[test]
fn nist_vector_abc() {
    let digest = sha256_digest(b"abc");
    assert_eq!(
        hex(&digest),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn nist_vector_two_block_message() {
    let msg = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    let digest = sha256_digest(msg);
    assert_eq!(
        hex(&digest),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn nist_vector_empty() {
    let digest = sha256_digest(b"");
    assert_eq!(
        hex(&digest),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn nist_vector_long_message() {
    // One million 'a' characters.
    let msg = vec![b'a'; 1_000_000];
    let digest = sha256_digest(&msg);
    assert_eq!(
        hex(&digest),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}
