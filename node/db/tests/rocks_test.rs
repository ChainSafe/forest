mod db_utils;

use db::{rocks::RocksDb, Read, Write};
use db_utils::DBPath;

#[test]
fn start() {
    let path = DBPath::new("start_rocks_test");
    RocksDb::open(path.as_ref()).unwrap();
}

#[test]
fn write() {
    let path = DBPath::new("write_rocks_test");
    let key = [1];
    let value = [1];

    let db: RocksDb = RocksDb::open(path.as_ref()).unwrap();
    db.write(key, value).unwrap();
}

#[test]
fn read() {
    let path = DBPath::new("read_rocks_test");
    let key = [0];
    let value = [1];
    let db = RocksDb::open(path.as_ref()).unwrap();
    db.write(key.clone(), value.clone()).unwrap();
    let res = db.read(key).unwrap().unwrap();
    assert_eq!(value.to_vec(), res);
}

#[test]
fn exists() {
    let path = DBPath::new("exists_rocks_test");
    let key = [0];
    let value = [1];
    let db = RocksDb::open(path.as_ref()).unwrap();
    db.write(key.clone(), value.clone()).unwrap();
    let res = db.exists(key).unwrap();
    assert_eq!(res, true);
}

#[test]
fn does_not_exist() {
    let path = DBPath::new("does_not_exists_rocks_test");
    let key = [0];
    let db = RocksDb::open(path.as_ref()).unwrap();
    let res = db.exists(key).unwrap();
    assert_eq!(res, false);
}

#[test]
fn delete() {
    let path = DBPath::new("delete_rocks_test");
    let key = [0];
    let value = [1];
    let db = RocksDb::open(path.as_ref()).unwrap();
    db.write(key.clone(), value.clone()).unwrap();
    let res = db.exists(key.clone()).unwrap();
    assert_eq!(res, true);
    db.delete(key.clone()).unwrap();
    let res = db.exists(key.clone()).unwrap();
    assert_eq!(res, false);
}

#[test]
fn bulk_write() {
    let path = DBPath::new("bulk_write_rocks_test");
    let keys = [[0], [1], [2]];
    let values = [[0], [1], [2]];
    let db = RocksDb::open(path.as_ref()).unwrap();
    db.bulk_write(&keys, &values).unwrap();
    for k in keys.iter() {
        let res = db.exists(k.clone()).unwrap();
        assert_eq!(res, true);
    }
}

#[test]
fn bulk_read() {
    let path = DBPath::new("bulk_read_rocks_test");
    let keys = [[0], [1], [2]];
    let values = [[0], [1], [2]];
    let db = RocksDb::open(path.as_ref()).unwrap();
    db.bulk_write(&keys, &values).unwrap();
    let results = db.bulk_read(&keys).unwrap();
    for (result, value) in results.iter().zip(values.iter()) {
        match result {
            Some(v) => assert_eq!(v, value),
            None => panic!("No values found!"),
        }
    }
}

#[test]
fn bulk_delete() {
    let path = DBPath::new("bulk_delete_rocks_test");
    let keys = [[0], [1], [2]];
    let values = [[0], [1], [2]];
    let db = RocksDb::open(path.as_ref()).unwrap();
    db.bulk_write(&keys, &values).unwrap();
    db.bulk_delete(&keys).unwrap();
    for k in keys.iter() {
        let res = db.exists(k.clone()).unwrap();
        assert_eq!(res, false);
    }
}
