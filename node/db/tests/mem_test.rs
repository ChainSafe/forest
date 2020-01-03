mod subtests;

use db::MemoryDB;

#[test]
fn start() {
    MemoryDB::open();
}

#[test]
fn write() {
    let db = MemoryDB::open();
    subtests::write(&db);
}

#[test]
fn read() {
    let db = MemoryDB::open();
    subtests::read(&db);
}

#[test]
fn exists() {
    let db = MemoryDB::open();
    subtests::exists(&db);
}

#[test]
fn does_not_exist() {
    let db = MemoryDB::open();
    subtests::does_not_exist(&db);
}

#[test]
fn delete() {
    let db = MemoryDB::open();
    subtests::delete(&db);
}

#[test]
fn bulk_write() {
    let db = MemoryDB::open();
    subtests::bulk_write(&db);
}

#[test]
fn bulk_read() {
    let db = MemoryDB::open();
    subtests::bulk_read(&db);
}

#[test]
fn bulk_delete() {
    let db = MemoryDB::open();
    subtests::bulk_delete(&db);
}
