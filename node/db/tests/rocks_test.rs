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

#[test]
fn exists() {
    let path = DBPath::new("exists_rocks_test");
    let key = vec![0];
    let value = vec![1];
    let db = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    if let Err(e) = RocksDb::write(&db, key.clone(), value.clone()) {
        panic!("{:?}", e);
    }
    match RocksDb::exists(&db, key) {
        Ok(res) => assert_eq!(res, true),
        Err(e) => panic!(e)
    }
}

#[test]
fn does_not_exist() {
    let path = DBPath::new("does_not_exists_rocks_test");
    let key = vec![0];
    let db = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    match RocksDb::exists(&db, key) {
        Ok(res) => assert_eq!(res, false),
        Err(e) => panic!(e)
    }
}

#[test]
fn delete() {
    let path = DBPath::new("delete_rocks_test");
    let key = vec![0];
    let value = vec![1];
    let db = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    if let Err(e) = RocksDb::write(&db, key.clone(), value.clone()) {
        panic!("{:?}", e);
    }
    match RocksDb::exists(&db, key.clone()) {
        Ok(res) => assert_eq!(res, true),
        Err(e) => panic!(e)
    }
    if let Err(e) = RocksDb::delete(&db, key.clone()) {
        panic!("{:?}", e);
    }
    match RocksDb::exists(&db, key.clone()) {
        Ok(res) => assert_eq!(res, false),
        Err(e) => panic!(e)
    }
}

#[test]
fn bulk_write() {
    let path = DBPath::new("bulk_write_rocks_test");
    let keys = [vec![0], vec![1], vec![2]];
    let values = [vec![0], vec![1], vec![2]];
    let db = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    if let Err(e) = RocksDb::bulk_write(&db, &keys, &values) {
        panic!("{:?}", e);
    };
    for k in keys.iter() {
        match RocksDb::exists(&db, k.clone()) {
            Ok(res) => assert_eq!(res, true),
            Err(e) => panic!(e)
        }
    }
}

#[test]
fn bulk_delete() {
    let path = DBPath::new("bulk_delete_rocks_test");
    let keys = [vec![0], vec![1], vec![2]];
    let values = [vec![0], vec![1], vec![2]];
    let db = match RocksDb::open(&path.path.as_path()) {
        Ok(db) => db,
        Err(e) => panic!("{:?}", e),
    };
    if let Err(e) = RocksDb::bulk_write(&db, &keys, &values) {
        panic!("{:?}", e);
    };
    if let Err(e) = RocksDb::bulk_delete(&db, &keys) {
        panic!("{:?}", e);
    }
    for k in keys.iter() {
        match RocksDb::exists(&db, k.clone()) {
            Ok(res) => assert_eq!(res, false),
            Err(e) => panic!(e)
        }
    }
}