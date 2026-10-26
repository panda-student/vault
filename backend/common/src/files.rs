//! Safe file moves preserve the source until copying and flushing succeed.
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};
pub fn move_file(source: &Path, target: &Path) -> std::io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(17) => copy_move(source, target),
        Err(error) => Err(error),
    }
}
pub fn copy_move(source: &Path, target: &Path) -> std::io::Result<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let copied = std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
    drop(output);
    drop(input);
    if let Err(error) = copied {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    if let Err(error) = fs::remove_file(source) {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirected_volume_fallback_preserves_bytes_and_does_not_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("old.exe");
        let target = directory.path().join("new.exe");
        fs::write(&source, b"MZtest").unwrap();
        fs::write(&target, b"existing").unwrap();
        assert!(copy_move(&source, &target).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"MZtest");
        fs::remove_file(&target).unwrap();
        copy_move(&source, &target).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"MZtest");
    }
}
