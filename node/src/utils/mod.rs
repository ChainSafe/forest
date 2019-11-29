#![allow(dead_code)]
extern crate dirs;

use dirs::home_dir;
use std::fs::{create_dir_all, remove_dir_all, File};
use std::io::{prelude::*, Result};
use std::path::Path;

// Generic methods

/// Writes a &str to a specified file.
///
/// # Argument
///
/// * `message` - &str contents to write to the file
/// * `path`    - String representation of where the file will be written to in the system
// TODO Do we assume the file does not exist?
fn write_string_to_file(message: &str, path: &str, file_name: &str) -> Result<()> {
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
fn get_home_dir() -> String {
    // We will panic if we cannot determine a home directory.
    home_dir().unwrap().to_str().unwrap().to_owned()
}

// libp2p specific methods

/// Returns a formatted String to the local libp2p directory
fn get_libp2p_path() -> String {
    // TODO remove hardcoded `file`
    let file = "/.ferret/libp2p";
    format!("{:?}{}", get_home_dir(), file)
}

// Test function
// Please use with caution, remove_dir_all will completely delete a directory
fn cleanup_file(path: &str) {
    match remove_dir_all(path) {
        Ok(_) => (),
        Err(e) => {
            println!("cleanup_file() failed: {:?}", e);
            assert!(false);
        }
    }
}

#[test]
fn test_write_string_to_file() {
    let path = "./test-write/";
    let file = "test.txt";
    match write_string_to_file("message", path, file) {
        Ok(_) => cleanup_file(path),
        Err(e) => {
            println!("{:?}", e);
            cleanup_file(path);
            assert!(false);
        }
    }
}

#[test]
fn test_write_string_to_file_nested_dir() {
    let root = "./test_missing";
    let path = format!("{}{}", root, "/test_write_string/");
    match write_string_to_file("message", &path, "test-file") {
        Ok(_) => cleanup_file(root),
        Err(e) => {
            println!("{:?}", e);
            cleanup_file(root);
            assert!(false);
        }
    }
}

#[test]
fn test_read_file() {
    let msg = "Hello World!";
    let path = "./test_read_file/";
    let file_name = "out.keystore";
    match write_string_to_file(msg, path, file_name) {
        Ok(_) => (),
        Err(e) => assert!(false, e),
    }
    match read_file(format!("{}{}", path, file_name)) {
        Ok(contents) => {
            cleanup_file(path);
            assert_eq!(contents, msg)
        }
        Err(e) => {
            println!("{:?}", e);
            cleanup_file(path);
            assert!(false);
        }
    }
}

#[test]
fn test_get_libp2p_path() {
    // This issue is OS specific testing is very difficult
    let path = get_libp2p_path();
    let ending = "/.ferret/libp2p";
    assert!(path.ends_with(ending));
}
