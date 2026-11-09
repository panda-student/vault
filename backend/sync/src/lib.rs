//! Remote sync: merging records and the stores that hold the encrypted sync file.
pub mod merge;
pub mod stores;

pub use stores::Stores;

/// Lowercase hex SHA-256, used for content hashes and file versions.
pub(crate) fn sha_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
