use super::invalid_update;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use vault_contracts::{errors::Result, updates::Manifest};

/// The signed installer must match the manifest size, MZ header and SHA-256.
pub(crate) fn verify_binary(path: &Path, manifest: &Manifest) -> Result<()> {
    let mut file = File::open(path).map_err(|_| invalid_update())?;
    if file.metadata().map_err(|_| invalid_update())?.len() != manifest.size {
        return Err(invalid_update());
    }
    let mut hasher = Sha256::new();
    let mut prefix = [0; 2];
    file.read_exact(&mut prefix).map_err(|_| invalid_update())?;
    if prefix != *b"MZ" {
        return Err(invalid_update());
    }
    hasher.update(prefix);
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|_| invalid_update())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if format!("{:x}", hasher.finalize()) != manifest.sha256.to_ascii_lowercase() {
        return Err(invalid_update());
    }
    Ok(())
}
