//! Safe file moves preserve the source until copying and flushing succeed.
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};
pub(crate) fn move_file(source: &Path, target: &Path) -> std::io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(17) => copy_move(source, target),
        Err(error) => Err(error),
    }
}
pub(crate) fn copy_move(source: &Path, target: &Path) -> std::io::Result<()> {
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
