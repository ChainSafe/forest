mod db_utils;
mod subtests;

use db::{DatabaseService, RocksDb};
use db_utils::DBPath;

#[test]
fn open() {
    let path = DBPath::new("start_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    subtests::open(&mut db);
    // Calling open on opened db should not error
    subtests::open(&mut db);
}

#[test]
fn write() {
    let path = DBPath::new("write_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::write(&db);
}

#[test]
fn read() {
    let path = DBPath::new("read_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::read(&db);
}

#[test]
fn exists() {
    let path = DBPath::new("exists_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::exists(&db);
}

#[test]
fn does_not_exist() {
    let path = DBPath::new("does_not_exists_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::does_not_exist(&db);
}

#[test]
fn delete() {
    let path = DBPath::new("delete_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::delete(&db);
}

#[test]
fn bulk_write() {
    let path = DBPath::new("bulk_write_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::bulk_write(&db);
}

#[test]
fn bulk_read() {
    let path = DBPath::new("bulk_read_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::bulk_read(&db);
}

#[test]
fn bulk_delete() {
    let path = DBPath::new("bulk_delete_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::bulk_delete(&db);
}
