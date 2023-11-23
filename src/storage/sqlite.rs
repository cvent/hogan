#![allow(clippy::from_over_into)]
use crate::storage::cache::Cache;

use super::cache;
use anyhow::Result;
use hogan::config::Environment;
use hogan::config::EnvironmentDescription;
use rusqlite::{params, Connection, OpenFlags};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SqliteCache {
    db_path: String,
}

impl SqliteCache {
    pub fn new(db_path: &str) -> Self {
        SqliteCache {
            db_path: db_path.to_string(),
        }
    }
}

impl Cache for SqliteCache {
    fn id(&self) -> &str {
        &self.db_path
    }

    fn clean(&self, max_age: usize) -> Result<()> {
        clean_db(&self.db_path, max_age).map(|_| ())
    }

    fn read_env(&self, env: &str, sha: &str) -> Result<Option<Arc<Environment>>> {
        read_sql_env(&self.db_path, env, sha)
    }

    fn write_env(&self, env: &str, sha: &str, data: &Environment) -> Result<()> {
        write_sql_env(&self.db_path, env, sha, data).map(|_| ())
    }

    fn read_env_listing(&self, sha: &str) -> Result<Option<Arc<Vec<EnvironmentDescription>>>> {
        read_sql_env_listing(&self.db_path, sha)
    }

    fn write_env_listing(&self, sha: &str, data: &[EnvironmentDescription]) -> Result<()> {
        write_sql_env_listing(&self.db_path, sha, data).map(|_| ())
    }
}

fn open_sql_db(db_path: &str, read_only: bool) -> Result<Connection> {
    let read_flag = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    };
    let conn =
        Connection::open_with_flags(db_path, read_flag | OpenFlags::SQLITE_OPEN_SHARED_CACHE)?;

    if !read_only {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS hogan (
            key STRING PRIMARY KEY,
            data BLOB,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP )",
            [],
        )?;
    }

    debug!("Opened sqlite connection to {}", db_path);

    Ok(conn)
}

fn clean_db(db_path: &str, db_max_age: usize) -> Result<usize> {
    info!(
        "Clearing db {} of all items older than {} days",
        db_path, db_max_age
    );
    let conn = open_sql_db(db_path, false)?;
    let mut query =
        conn.prepare("DELETE FROM hogan WHERE timestamp < date('now', '-' || ? || ' days')")?;
    let data = query.execute([db_max_age])?;
    info!("Cleaned database, removed: {} rows", data);
    Ok(data)
}

fn read_sql_env(db_path: &str, env: &str, sha: &str) -> Result<Option<Arc<Environment>>> {
    let conn = open_sql_db(db_path, true)?;
    let mut query = conn.prepare("SELECT data FROM hogan WHERE key LIKE ? || '%' LIMIT 1")?;
    let key = gen_env_key(sha, env);
    let data: Option<rusqlite::Result<Vec<u8>>> =
        query.query_map(params![key], |row| row.get(0))?.next();
    if let Some(data) = data {
        let deserialized_data = cache::deserialize_env(data?)?;
        Ok(Some(Arc::new(deserialized_data)))
    } else {
        debug!("Unable to find {} in sqlite db", key);
        Ok(None)
    }
}

fn write_sql_env(db_path: &str, env: &str, sha: &str, data: &Environment) -> Result<usize> {
    let conn = open_sql_db(db_path, false)?;
    let key = gen_env_key(sha, env);
    let serialized_data = cache::serialize_env(data)?;

    conn.execute(
        "INSERT INTO hogan (key, data) VALUES (?1, ?2)",
        params![key, serialized_data],
    )
    .map_err(|e| e.into())
}

fn gen_env_key(sha: &str, env: &str) -> String {
    format!("{}::{}", env, sha)
}

fn gen_env_listing_key(sha: &str) -> String {
    format!("!listing::{}", sha)
}

fn write_sql_env_listing(
    db_path: &str,
    sha: &str,
    data: &[EnvironmentDescription],
) -> Result<usize> {
    let conn = open_sql_db(db_path, false)?;
    let key = gen_env_listing_key(sha);
    let serialized_data = cache::serialize_env_listing(data)?;

    conn.execute(
        "INSERT INTO hogan (key, data) VALUES (?1, ?2)",
        params![key, serialized_data],
    )
    .map_err(|e| e.into())
}

fn read_sql_env_listing(
    db_path: &str,
    sha: &str,
) -> Result<Option<Arc<Vec<EnvironmentDescription>>>> {
    let conn = open_sql_db(db_path, true)?;
    let mut query = conn.prepare("SELECT data FROM hogan WHERE key LIKE ? || '%' LIMIT 1")?;
    let key = gen_env_listing_key(sha);
    let data: Option<rusqlite::Result<Vec<u8>>> =
        query.query_map(params![key], |row| row.get(0))?.next();
    if let Some(data) = data {
        let deserialized_data = cache::deserialize_env_listing(data?)?;
        Ok(Some(Arc::new(deserialized_data)))
    } else {
        debug!("Unable to find {} in sqlite db", key);
        Ok(None)
    }
}
