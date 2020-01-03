mod subtests;

use db::MemoryDB;

#[test]
fn start() {
    MemoryDB::default();
}

#[test]
fn write() {
    let db = MemoryDB::default();
    subtests::write(&db);
}

#[test]
fn read() {
    let db = MemoryDB::default();
    subtests::read(&db);
}

#[test]
fn exists() {
    let db = MemoryDB::default();
    subtests::exists(&db);
}

#[test]
fn does_not_exist() {
    let db = MemoryDB::default();
    subtests::does_not_exist(&db);
}

#[test]
fn delete() {
    let db = MemoryDB::default();
    subtests::delete(&db);
}

#[test]
fn bulk_write() {
    let db = MemoryDB::default();
    subtests::bulk_write(&db);
}

#[test]
fn bulk_read() {
    let db = MemoryDB::default();
    subtests::bulk_read(&db);
}

#[test]
fn bulk_delete() {
    let db = MemoryDB::default();
    subtests::bulk_delete(&db);
}
