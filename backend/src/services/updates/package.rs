use super::*;
pub(crate) fn verify_binary(path: &Path, manifest: &Manifest) -> Result<()> {
    verify_file(path, manifest.size, &manifest.sha256, b"MZ")
}
pub(crate) fn verify_file(path: &Path, size: u64, hash: &str, magic: &[u8; 2]) -> Result<()> {
    let mut file = File::open(path).map_err(|_| invalid_update())?;
    if file.metadata().map_err(|_| invalid_update())?.len() != size {
        return Err(invalid_update());
    }
    let mut hasher = Sha256::new();
    let mut prefix = [0; 2];
    file.read_exact(&mut prefix).map_err(|_| invalid_update())?;
    if prefix != *magic {
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
    if format!("{:x}", hasher.finalize()) != hash.to_ascii_lowercase() {
        return Err(invalid_update());
    }
    Ok(())
}

/// Read a single fixed member into a new file; archive paths never become destinations.
pub(crate) fn extract_update_payload(
    archive: &Path,
    output: &Path,
    member: &str,
    size: u64,
) -> Result<()> {
    let file = File::open(archive).map_err(|_| invalid_update())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| invalid_update())?;
    let index = zip
        .file_names()
        .position(|name| name == member)
        .ok_or_else(invalid_update)?;
    let mut entry = zip.by_index(index).map_err(|_| invalid_update())?;
    if entry.size() != size || !entry.is_file() {
        return Err(invalid_update());
    }
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|_| invalid_update())?;
    let count = std::io::copy(&mut entry.by_ref().take(size + 1), &mut destination)
        .map_err(|_| invalid_update())?;
    if count != size {
        return Err(invalid_update());
    }
    destination.sync_all().map_err(|_| AppError::storage())?;
    Ok(())
}
