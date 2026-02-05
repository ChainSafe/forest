// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! Ported from <https://github.com/filecoin-project/lotus/blob/v1.34.3/lib/sqlite/sqlite.go>
//!

#![allow(dead_code)]

#[cfg(test)]
mod tests;

use anyhow::Context as _;
use sqlx::{
    SqlitePool,
    query::Query,
    sqlite::{
        SqliteArguments, SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode,
        SqliteSynchronous,
    },
};
use std::{cmp::Ordering, path::Path, time::Instant};

pub type SqliteQuery<'q> = Query<'q, sqlx::Sqlite, SqliteArguments<'q>>;

/// Opens for creates a database at the specified path
pub async fn open_file(file: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(dir) = file.parent()
        && !dir.is_dir()
    {
        std::fs::create_dir_all(dir)?;
    }
    let options = SqliteConnectOptions::new().filename(file);
    Ok(open(options).await?)
}

/// Opens for creates an in-memory database
pub async fn open_memory() -> sqlx::Result<SqlitePool> {
    open(
        SqliteConnectOptions::new()
            .in_memory(true)
            .shared_cache(true),
    )
    .await
}

/// Opens a database at the given path. If the database does not exist, it will be created.
pub async fn open(options: SqliteConnectOptions) -> sqlx::Result<SqlitePool> {
    let options = options
        .synchronous(SqliteSynchronous::Normal)
        .pragma("temp_store", "memory")
        .pragma("mmap_size", "30000000000")
        .auto_vacuum(SqliteAutoVacuum::None)
        .journal_mode(SqliteJournalMode::Wal)
        .pragma("journal_size_limit", "0") // always reset journal and wal files
        .foreign_keys(true)
        .read_only(false);
    SqlitePool::connect_with(options).await
}

/// This function initializes the database by checking whether it needs to be created or upgraded.
/// The `ddls` are the `DDL`(Data Definition Language) statements to create the tables in the database and their initial required
/// content. The `schema_version` will be set inside the database if it is newly created. Otherwise, the
/// version is read from the database and returned. This value should be checked against the expected
/// version to determine if the database needs to be upgraded.
/// It is up to the caller to close the database if an error is returned by this function.
pub async fn init_db<'q>(
    db: &SqlitePool,
    name: &str,
    ddls: impl IntoIterator<Item = SqliteQuery<'q>>,
    version_migrations: Vec<SqliteQuery<'q>>,
) -> anyhow::Result<()> {
    let schema_version = version_migrations.len() + 1;

    let init = async |db: &SqlitePool, schema_version| {
        let mut tx = db.begin().await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS _meta (version UINT64 NOT NULL UNIQUE)")
            .execute(tx.as_mut())
            .await?;
        for i in 1..=schema_version {
            sqlx::query("INSERT OR IGNORE INTO _meta (version) VALUES (?)")
                .bind(i as i64)
                .execute(tx.as_mut())
                .await?;
        }
        for ddl in ddls.into_iter() {
            ddl.execute(tx.as_mut()).await?;
        }
        tx.commit().await
    };

    if sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='_meta';")
        .fetch_optional(db)
        .await
        .map_err(|e| anyhow::anyhow!("error looking for {name} database _meta table: {e}"))?
        .is_none()
    {
        init(db, schema_version).await?;
    }

    let found_version: u64 = sqlx::query_scalar("SELECT max(version) FROM _meta")
        .fetch_optional(db)
        .await?
        .with_context(|| format!("invalid {name} database version: no version found"))?;
    anyhow::ensure!(found_version > 0, "schema version should be 1 based");

    let run_vacuum = match found_version.cmp(&(schema_version as _)) {
        Ordering::Greater => {
            anyhow::bail!(
                "invalid {name} database version: version {found_version} is greater than the number of migrations {schema_version}"
            );
        }
        Ordering::Equal => false,
        Ordering::Less => true,
    };

    // run a migration for each version that we have not yet applied, where `found_version` is what is
    // currently in the database and `schema_version` is the target version. If they are the same,
    // nothing is run.

    for (from_version, to_version, migration) in version_migrations
        .into_iter()
        .enumerate()
        .map(|(i, m)| (i + 1, i + 2, m))
        // versions start at 1, but the migrations are 0-indexed where the first migration would take us to version 2
        .skip(found_version as usize - 1)
    {
        tracing::info!("Migrating {name} database to version {to_version}");
        let now = Instant::now();
        let mut tx = db.begin().await?;
        migration.execute(tx.as_mut()).await?;
        sqlx::query("INSERT OR IGNORE INTO _meta (version) VALUES (?)")
            .bind(to_version as i64)
            .execute(tx.as_mut())
            .await?;
        tx.commit().await?;
        tracing::info!(
            "Successfully migrated {name} database from version {from_version} to {to_version} in {}",
            humantime::format_duration(now.elapsed())
        );
    }

    if run_vacuum {
        // During the large migrations, we have likely increased the WAL size a lot, so lets do some
        // simple DB administration to free up space (VACUUM followed by truncating the WAL file)
        // as this would be a good time to do it when no other writes are happening.
        tracing::info!(
            "Performing {name} database vacuum and wal checkpointing to free up space after the migration"
        );
        if let Err(e) = sqlx::query("VACUUM").execute(db).await {
            tracing::warn!("error vacuuming {name} database: {e}")
        }
        if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(db)
            .await
        {
            tracing::warn!("error checkpointing {name} database wal: {e}")
        }
    }

    Ok(())
}
