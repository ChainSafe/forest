// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use dirs::home_dir;
use std::fs::{create_dir_all, File};
use std::io::{prelude::*, Result};
use std::path::Path;
use toml;

/// Writes a string to a specified file. Creates the desired path if it does not exist.
/// Note: `path` and `filename` are appended to produce the resulting file path.
pub fn write_to_file(message: &[u8], path: &str, file_name: &str) -> Result<()> {
    // Create path if it doesn't exist
    create_dir_all(Path::new(path))?;
    let join = format!("{}{}", path, file_name);
    let mut file = File::create(join)?;
    file.write_all(message)?;
    Ok(())
}

/// Read file as a `Vec<u8>`
pub fn read_file_to_vec(path: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Read file as a `String`.
pub fn read_file_to_string(path: &str) -> Result<String> {
    let mut file = File::open(path)?;
    let mut string = String::new();
    file.read_to_string(&mut string)?;
    Ok(string)
}

/// Gets the home directory of the current system.
/// Will return correct path for windows, linux, and osx.
///
/// # Panics
/// We will panic if we cannot determine a home directory.
pub fn get_home_dir() -> String {
    home_dir().unwrap().to_str().unwrap().to_owned()
}

/// Converts a toml file represented as a string to `S`
///
/// # Example
/// ```
/// use serde::Deserialize;
/// use utils::read_toml;
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
pub fn read_toml<S>(toml_string: &str) -> Result<S>
where
    for<'de> S: serde::de::Deserialize<'de>,
{
    let new_struct: S = toml::from_str(toml_string)?;
    Ok(new_struct)
}
