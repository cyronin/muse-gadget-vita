//! Whole files on the memory card, in the app's own folder.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Where the app keeps everything: the avatar, the pairing, the keys and
/// the log. tools/install.py copies them here.
pub const HOME: &str = "ux0:data/musevita/";

fn path(name: &str) -> PathBuf {
    PathBuf::from(HOME).join(name)
}

/// Makes the app's folders, in case nothing has been installed yet.
pub fn prepare() {
    let _ = fs::create_dir_all(path("state"));
}

/// Adds to the end of the file, creating it if need be.
pub fn append(name: &str, bytes: &[u8]) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path(name)) {
        let _ = file.write_all(bytes);
    }
}

pub fn read(name: &str) -> Option<Vec<u8>> {
    fs::read(path(name)).ok()
}

/// Replaces the file. True when every byte was written; callers that must
/// know it reached the card read it back.
pub fn write(name: &str, bytes: &[u8]) -> bool {
    let Ok(mut file) = fs::File::create(path(name)) else {
        return false;
    };
    file.write_all(bytes).is_ok()
}

pub fn remove(name: &str) {
    let _ = fs::remove_file(path(name));
}

pub fn rename(from: &str, to: &str) -> bool {
    fs::rename(path(from), path(to)).is_ok()
}
