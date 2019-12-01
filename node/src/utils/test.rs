#![cfg(all(test))]
use crate::utils::*;

use std::fs::remove_dir_all;

// Please use with caution, remove_dir_all will completely delete a directory
fn cleanup_file(path: &str) {
    if let Err(e) = remove_dir_all(path) {
        println!("cleanup_file() failed: {:?}", e);
        panic!(false);
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
