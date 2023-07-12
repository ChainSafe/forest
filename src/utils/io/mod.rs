// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod progress_bar;
pub mod progress_log;
mod tempfile;
mod writer_checksum;

use std::{
    fs::{create_dir_all, File},
    io::{prelude::*, Result},
    path::Path,
};

pub use progress_bar::{ProgressBar, ProgressBarVisibility};
pub use progress_log::{WithProgress, WithProgressRaw};
pub use writer_checksum::*;

pub use self::tempfile::*;

/// Restricts permissions on a file to user-only: 0600
#[cfg(unix)]
pub fn set_user_perm(file: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    use log::info;

    let mut perm = file.metadata()?.permissions();
    #[allow(clippy::useless_conversion)] // Otherwise it does not build on macos
    perm.set_mode((libc::S_IWUSR | libc::S_IRUSR).into());
    file.set_permissions(perm)?;

    info!("Permissions set to 0600 on {:?}", file);

    Ok(())
}

#[cfg(not(unix))]
pub fn set_user_perm(file: &File) -> Result<()> {
    Ok(())
}

/// Writes a string to a specified file. Creates the desired path if it does not
/// exist. Note: `path` and `filename` are appended to produce the resulting
/// file path.
pub fn write_to_file(message: &[u8], path: &Path, file_name: &str) -> Result<File> {
    // Create path if it doesn't exist
    create_dir_all(path)?;
    let mut file = File::create(path.join(file_name))?;
    file.write_all(message)?;
    Ok(file)
}

/// Read file as a `Vec<u8>`
pub fn read_file_to_vec(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Read file as a `String`.
pub fn read_file_to_string(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut string = String::new();
    file.read_to_string(&mut string)?;
    Ok(string)
}

/// Converts a TOML file represented as a string to `S`
///
/// # Example
/// ```
/// use serde::Deserialize;
/// use forest_filecoin::doctest_private::read_toml;
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
