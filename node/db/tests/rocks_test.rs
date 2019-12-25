use db::{rocks::RocksDb, traits::Write};
use std::fs::remove_dir_all;
use std::path::Path;

// Please use with caution, remove_dir_all will completely delete a directory
fn cleanup_file(path: &str) {
    if let Err(e) = remove_dir_all(path) {
        panic!("cleanup_file() path: {:?} failed: {:?}", path, e);
    }
}

#[test]
fn open() {
    let path = Path::new("./test-db");
    if let Err(e) = RocksDb::open(path) {
        panic!("{:?}", e);
    };
    cleanup_file("./test-db");
}

#[test]
fn write() {
    let path = Path::new("./test-db");
    let key = vec![1];
    let value = vec![1];

    cleanup_file("./test-db");

    let db: RocksDb = match RocksDb::open(path) {
        Ok(db) => {
            println!("here");
            db
        }
        Err(e) => {
            println!("here2");
            panic!("{:?}", e);
        }
    };
    println!("{:?}", &db);

    match RocksDb::write(&db, key, value) {
        Ok(_) => cleanup_file("./test-db"),
        Err(e) => {
            panic!("{:?}", e);
        }
    }
}
