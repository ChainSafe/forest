// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use itertools::Itertools as _;
use sqlx::{FromRow as _, Row};

// Ported from <https://github.com/filecoin-project/lotus/blob/v1.34.3/lib/sqlite/sqlite_test.go#L16>
#[sqlx::test]
async fn test_sqlite() {
    let ddls = [
        "CREATE TABLE IF NOT EXISTS blip (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			blip_name TEXT NOT NULL
		)",
        "CREATE TABLE IF NOT EXISTS bloop (
		 	blip_id INTEGER NOT NULL,
			bloop_name TEXT NOT NULL,
			FOREIGN KEY (blip_id) REFERENCES blip(id)
		 )",
        "CREATE INDEX IF NOT EXISTS blip_name_index ON blip (blip_name)",
    ];

    let temp_db_path = tempfile::Builder::new()
        .suffix(".sqlite3")
        .tempfile_in(std::env::temp_dir())
        .unwrap();
    let db = open_file(temp_db_path.path()).await.unwrap();
    init_db(&db, "testdb", ddls.into_iter().map(sqlx::query), vec![])
        .await
        .unwrap();

    // insert some data
    let r = sqlx::query("INSERT INTO blip (blip_name) VALUES ('blip1')")
        .execute(&db)
        .await
        .unwrap();
    let id = r.last_insert_rowid();
    assert_eq!(id, 1);
    sqlx::query("INSERT INTO bloop (blip_id, bloop_name) VALUES (?, 'bloop1')")
        .bind(id)
        .execute(&db)
        .await
        .unwrap();
    let r = sqlx::query("INSERT INTO blip (blip_name) VALUES ('blip2')")
        .execute(&db)
        .await
        .unwrap();
    let id = r.last_insert_rowid();
    assert_eq!(id, 2);
    sqlx::query("INSERT INTO bloop (blip_id, bloop_name) VALUES (?, 'bloop2')")
        .bind(id)
        .execute(&db)
        .await
        .unwrap();

    let expected_indexes = vec!["blip_name_index".to_string()];
    let expected_data = vec![
        TableData {
            name: "_meta".to_string(),
            cols: vec![],
            data: vec![vec![Value::Number(1)]],
        },
        TableData {
            name: "blip".to_string(),
            cols: vec!["id".to_string(), "blip_name".to_string()],
            data: vec![
                vec![Value::Number(1), Value::String("blip1".to_string())],
                vec![Value::Number(2), Value::String("blip2".to_string())],
            ],
        },
        TableData {
            name: "bloop".to_string(),
            cols: vec!["blip_id".to_string(), "bloop_name".to_string()],
            data: vec![
                vec![Value::Number(1), Value::String("bloop1".to_string())],
                vec![Value::Number(2), Value::String("bloop2".to_string())],
            ],
        },
    ];

    // check that the db contains what we think it should
    let (indexes, data) = dump_tables(&db).await.unwrap();
    assert_eq!(indexes, expected_indexes);
    assert_eq!(data, expected_data);

    drop(db);

    // open again, check contents is the same
    let db = open_file(temp_db_path.path()).await.unwrap();
    init_db(&db, "testdb", ddls.into_iter().map(sqlx::query), vec![])
        .await
        .unwrap();
    let (indexes, data) = dump_tables(&db).await.unwrap();
    assert_eq!(indexes, expected_indexes);
    assert_eq!(data, expected_data);

    drop(db);

    // open again, with a migration
    let db = open_file(temp_db_path.path()).await.unwrap();
    let migration1 =
        sqlx::query("ALTER TABLE blip ADD COLUMN blip_extra TEXT NOT NULL DEFAULT '!'");
    init_db(
        &db,
        "testdb",
        ddls.into_iter().map(sqlx::query),
        vec![migration1],
    )
    .await
    .unwrap();

    // also add something new
    let r = sqlx::query("INSERT INTO blip (blip_name, blip_extra) VALUES ('blip1', '!!!')")
        .execute(&db)
        .await
        .unwrap();
    let id = r.last_insert_rowid();
    sqlx::query("INSERT INTO bloop (blip_id, bloop_name) VALUES (?, 'bloop3')")
        .bind(id)
        .execute(&db)
        .await
        .unwrap();

    // database should contain new stuff
    let mut expected_data = expected_data.clone();
    expected_data[0].data.push(vec![Value::Number(2)]);
    expected_data[1] = TableData {
        name: "blip".to_string(),
        cols: vec![
            "id".to_string(),
            "blip_name".to_string(),
            "blip_extra".to_string(),
        ],
        data: vec![
            vec![
                Value::Number(1),
                Value::String("blip1".to_string()),
                Value::String("!".to_string()),
            ],
            vec![
                Value::Number(2),
                Value::String("blip2".to_string()),
                Value::String("!".to_string()),
            ],
            vec![
                Value::Number(3),
                Value::String("blip1".to_string()),
                Value::String("!!!".to_string()),
            ],
        ],
    };
    expected_data[2]
        .data
        .push(vec![Value::Number(3), Value::String("bloop3".to_string())]);
    let (indexes, data) = dump_tables(&db).await.unwrap();
    assert_eq!(indexes, expected_indexes);
    assert_eq!(data, expected_data);
}

async fn dump_tables(db: &SqlitePool) -> anyhow::Result<(Vec<String>, Vec<TableData>)> {
    let rows = sqlx::query("SELECT name FROM sqlite_master WHERE type='index'")
        .fetch_all(db)
        .await?;
    let indexes: Vec<_> = rows
        .into_iter()
        .filter_map(|r| {
            let n: sqlx::Result<String> = r.try_get(0);
            match n {
                Ok(n) => {
                    if n.contains("sqlite_autoindex") {
                        None
                    } else {
                        Some(Ok(n))
                    }
                }
                Err(e) => Some(Err(e)),
            }
        })
        .try_collect()?;

    let rows = sqlx::query("SELECT name, sql FROM sqlite_master WHERE type = 'table'")
        .fetch_all(db)
        .await?;
    let mut data = Vec::with_capacity(rows.len());
    for r in rows.iter() {
        let tr = TableDataRow::from_row(r)?;
        if !tr.name.starts_with("sqlite") {
            data.push(TableData::try_from(tr, db).await?);
        }
    }

    Ok((indexes, data))
}

#[derive(sqlx::FromRow, Debug)]
struct TableDataRow {
    name: String,
    sql: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum Value {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct TableData {
    name: String,
    cols: Vec<String>,
    data: Vec<Vec<Value>>,
}

impl TableData {
    async fn try_from(
        TableDataRow { name, sql }: TableDataRow,
        db: &SqlitePool,
    ) -> anyhow::Result<Self> {
        let mut cols = vec![];
        for s in sql.split("\n") {
            // alter table does funky things to the sql, hence the "," ReplaceAll:
            match s
                .replace(",", "")
                .trim()
                .split(" ")
                .next()
                .context("infallible")?
            {
                "CREATE" | "FOREIGN" | "" | ")" => {}
                s => cols.push(s.to_string()),
            }
        }
        let mut data = vec![];
        for r in sqlx::query(&format!("SELECT * FROM {name}"))
            .fetch_all(db)
            .await?
        {
            let len = r.columns().len();
            let mut array = Vec::with_capacity(len);
            for i in 0..len {
                let id: sqlx::Result<i64> = r.try_get(i);
                match id {
                    Ok(id) => array.push(Value::Number(id)),
                    _ => {
                        let s: String = r.try_get(i)?;
                        array.push(Value::String(s));
                    }
                }
            }
            data.push(array);
        }
        Ok(Self { name, cols, data })
    }
}
