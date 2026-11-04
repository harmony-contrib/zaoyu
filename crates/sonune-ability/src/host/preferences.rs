//! Bounded reads and atomic writes for the sandbox's UI preferences.

use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

const MAX_BYTES: u64 = 4 * 1024 * 1024;

pub(super) fn read(directory: &Path) -> Option<Vec<u8>> {
    let file = match File::open(directory.join("ui.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!("could not read ui.json: {error}");
            return None;
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = file.take(MAX_BYTES + 1).read_to_end(&mut bytes) {
        log::warn!("could not read ui.json: {error}");
        return None;
    }
    if bytes.len() as u64 > MAX_BYTES {
        log::warn!("could not read ui.json: preferences exceed 4 MiB");
        return None;
    }
    Some(bytes)
}

pub(super) fn write(directory: &Path, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "preferences exceed 4 MiB"));
    }
    std::fs::create_dir_all(directory)?;
    let temporary = directory.join("ui.json.tmp");
    let result = (|| -> io::Result<()> {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, directory.join("ui.json"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
