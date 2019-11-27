extern crate dirs;

use std::fs::{File, create_dir_all};
use std::path::Path;
use std::io::{Result, prelude::*};
use dirs::home_dir;

// Generic methods

/// Writes a &str to a specified file.
/// 
/// # Argument
/// 
/// * `message` - &str contents to write to the file
/// * `path`    - String representation of where the file will be written to in the system
// TODO Do we assume the file exists?
fn write_string_to_file(message: &str, path: String) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(&message.as_bytes())?;
    Ok(())
}

/// Gets the home directory of the current system.
/// Will return correct path for windows, linux, and osx
fn get_home_dir() -> String {
    home_dir().unwrap().to_str().unwrap().to_owned()
}

// libp2p specific methods

/// Returns a formatted String to the local libp2p directory
fn get_libp2p_path() -> String {
    // TODO remove hardcoded `file`
    let file = "/ferret/libp2p";
    format!("{:?}{}", get_home_dir(), file)
}

/// Stores the libp2p id to the Ferret directory
pub fn write_libp2p_id(id: &str) -> Result<()> {
    let path = get_libp2p_path();
    
    // Create path if it doesn't exist
    if !Path::new(&path).exists() {
        create_dir_all(Path::new(&path))?;
    }
    // TODO handle result somehow
    write_string_to_file(id, get_libp2p_path())
}

/// Check if libp2p id exists in filesystem
pub fn get_libp2p_id() -> Result<String> {
    let mut file = File::open(get_libp2p_path())?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    // assert_eq!(contents, "Hello, world!");
    Ok(contents)
}