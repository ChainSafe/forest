mod db_utils;

use db::{rocks::RocksDb, traits::Write, traits::Read};
use db_utils::{DBPath};

#[test]
fn open() {
    let path = DBPath::new("open_rocks_test");
    if let Err(e) = RocksDb::open(&path.path.as_path()) {
        // cleanup_file("./test-db");
        panic!("{:?}", e);
    };
}

#[test]
fn write() {
    let path = DBPath::new("write_rocks_test");
    
    let key = vec![1];
    let value = vec![1];

    let db: RocksDb = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    println!("{:?}", &db);

    if let Err(e) = RocksDb::write(&db, key, value) {
        panic!("{:?}", e);
    }
}

#[test]
fn read() {
    let path = DBPath::new("read_rocks_test");
    let key = vec![0];
    let value = vec![1];
    let db = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    if let Err(e) = RocksDb::write(&db, key.clone(), value.clone()) {
        panic!("{:?}", e);
    }
    match RocksDb::read(&db, key) {
        Ok(res) => {
            assert_eq!(value, res);
        },
        Err(e) => panic!("{:?}", e),
    }
}