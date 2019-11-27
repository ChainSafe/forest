extern crate dirs;

use std::fs::{File, create_dir_all, remove_file};
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
// TODO Do we assume the file does not exist?
fn write_string_to_file(message: &str, path: &str) -> Result<()> {
    let mut file = File::create(path.to_owned())?;
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
    let file = "/.qferret/libp2p";
    format!("{:?}{}", get_home_dir(), file)
}

/// Stores the libp2p id to the Ferret directory
pub fn write_libp2p_id(id: &str) -> Result<()> {
    let path = get_libp2p_path();
    
    // Create path if it doesn't exist
    if !Path::new(&path).exists() {
        create_dir_all(Path::new(&path))?;
    }

    write_string_to_file(id, &get_libp2p_path())
}

/// Check if libp2p id exists in filesystem
pub fn get_libp2p_id() -> Result<String> {
    let mut file = File::open(get_libp2p_path())?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[test]
fn test_write_string_to_file() {
    let file_path = "./test.txt";
    match write_string_to_file("message", file_path) {
        Ok(_) => {
            // Cleanup file
            remove_file(file_path);
            assert!(true)
        },
        Err(_) => assert!(false),
    }
}

#[test]
fn test_write_and_read() {
    // let peer_id = "21312-312d-dq123-dd";
    // match write_libp2p_id(peer_id) {
    //     Ok(_) => assert!(true),
    //     Err(e) => assert!(false, e),
    // }

    // match get_libp2p_id() {
        // Ok(s) => assert_eq!(s, peer_id),
        // Err(e) => assert!(false, e),
    // }
}