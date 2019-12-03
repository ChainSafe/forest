use dirs::home_dir;
use serde;
use std::fs::{create_dir_all, File};
use std::io::{prelude::*, Result};
use std::path::Path;
use toml;

/// Writes a &str to a specified file.
///
/// # Argument
///
/// * `message`   - Contents that will be written to the file
/// * `path`      - Filesystem path of where the file will be written to
/// * `file_name` - Desired filename
pub fn write_string_to_file(message: &str, path: &str, file_name: &str) -> Result<()> {
    // Create path if it doesn't exist
    create_dir_all(Path::new(&path))?;
    let join = format!("{}{}", path, file_name);
    let mut file = File::create(join.to_owned())?;
    file.write_all(&message.as_bytes())?;
    Ok(())
}

/// Read file if it exists in the filesystem
///
/// # Arguments
///
/// * `path` - A String representing the path to a file
pub fn read_file(path: String) -> Result<String> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Gets the home directory of the current system.
/// Will return correct path for windows, linux, and osx
pub fn get_home_dir() -> String {
    // We will panic if we cannot determine a home directory.
    home_dir().unwrap().to_str().unwrap().to_owned()
}

/// Converts a toml
///
/// # Arguments
///
/// * `&str` - &str represenation of a toml file
///
/// # Example
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String
/// };
///
/// let path = "./config.toml".to_owned();
/// let toml_string = read_file(path).unwrap();
/// read_toml(toml_string)
pub fn read_toml<S>(toml_string: &str) -> Result<S>
where
    for<'de> S: serde::de::Deserialize<'de>,
{
    let new_struct: S = toml::from_str(toml_string)?;
    Ok(new_struct)
}
