// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod mmap;
pub mod progress_log;
mod writer_checksum;

use std::{
    fs::File,
    io::{self, prelude::*, Result},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

pub use mmap::EitherMmapOrRandomAccessFile;
pub use progress_log::WithProgress;
pub use writer_checksum::*;

/// Writes bytes to a specified file. Creates the desired path if it does not
/// exist.
/// Note: The file is created with permissions 0600.
/// Note: The file is truncated if it already exists.
pub fn write_new_sensitive_file(message: &[u8], path: &Path) -> Result<()> {
    create_new_sensitive_file(path)?.write_all(message)
}

/// Creates a new file with the specified path. The file is created
/// with permissions 0600 and is truncated if it already exists.
pub fn create_new_sensitive_file(path: &Path) -> Result<File> {
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Parent directory not found"))?,
    )?;

    let file = std::fs::OpenOptions::new()
        .mode(0o600)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;

    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

/// Converts a TOML file represented as a string to `S`
///
/// # Example
/// ```
/// use serde::Deserialize;
/// use forest::doctest_private::read_toml;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String
/// };
///
/// let toml_string = "name = \"forest\"\n";
/// let config: Config = read_toml(toml_string).unwrap();
/// assert_eq!(config.name, "forest");
/// ```
pub fn read_toml<S>(toml_string: &str) -> anyhow::Result<S>
where
    for<'de> S: serde::de::Deserialize<'de>,
{
    let new_struct: S = toml::from_str(toml_string)?;
    Ok(new_struct)
}

// When reading a password or prompting a yes/no answer, we change the terminal
// settings by hiding the cursor or turning off the character echo.
// Unfortunately, if we're interrupted, these settings are not restored. This
// function attempts to restore terminal settings to sensible values. However,
// if SIGKILL kills Forest, there's nothing we can do, and it'll be up to the
// user to restore their terminal.
pub fn terminal_cleanup() {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        use termios::*;
        let fd = std::io::stdin().as_raw_fd();
        if let Ok(mut termios) = Termios::from_fd(fd) {
            termios.c_lflag |= ECHO;
            let _ = tcsetattr(fd, TCSAFLUSH, &termios);
        }
    }
    let mut stdout = std::io::stdout();
    let _ = anes::execute!(&mut stdout, anes::ShowCursor);
}
