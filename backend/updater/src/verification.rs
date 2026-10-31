use super::*;
pub(crate) fn decode(envelope: &Envelope, key: &str, current: &str) -> Result<Manifest> {
    let public: [u8; 32] = STANDARD
        .decode(key.trim())
        .map_err(|_| invalid_update())?
        .try_into()
        .map_err(|_| invalid_update())?;
    let bytes = STANDARD
        .decode(&envelope.payload)
        .map_err(|_| invalid_update())?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(&envelope.signature)
            .map_err(|_| invalid_update())?,
    )
    .map_err(|_| invalid_update())?;
    VerifyingKey::from_bytes(&public)
        .map_err(|_| invalid_update())?
        .verify_strict(&bytes, &signature)
        .map_err(|_| invalid_update())?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| invalid_update())?;
    let version = semver::Version::parse(&manifest.version).map_err(|_| invalid_update())?;
    let current = semver::Version::parse(current).map_err(|_| invalid_update())?;
    if manifest.schema != 3
        || manifest.platform != "windows-x86_64"
        || version <= current
        || !version.pre.is_empty()
        || manifest.size < 2
        || manifest.size > MAX_PACKAGE
        || manifest.notes.len() > 16000
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid_update());
    }
    let expected = format!(
        "https://github.com/panda-student/vault/releases/download/v{0}/vault-{0}-setup.exe",
        manifest.version
    );
    if manifest.url != expected {
        return Err(invalid_update());
    }
    Ok(manifest)
}
