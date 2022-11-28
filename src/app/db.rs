#![allow(clippy::from_over_into)]

use anyhow::Result;
use compression::prelude::*;
use hogan::config::Environment;
use hogan::config::EnvironmentDescription;
use riker::actors::*;
use rusqlite::{params, Connection, OpenFlags};
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use crate::app::datadogstatsd::CustomMetrics;

use super::datadogstatsd::DdMetrics;

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

pub fn clean_db(db_path: &str, db_max_age: usize) -> Result<usize> {
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

pub fn read_sql_env(db_path: &str, env: &str, sha: &str) -> Result<Option<Environment>> {
    let conn = open_sql_db(db_path, true)?;
    let mut query = conn.prepare("SELECT data FROM hogan WHERE key LIKE ? || '%' LIMIT 1")?;
    let key = gen_env_key(sha, env);
    let data: Option<rusqlite::Result<Vec<u8>>> =
        query.query_map(params![key], |row| row.get(0))?.next();
    if let Some(data) = data {
        let decompressed_data = data?
            .into_iter()
            .decode(&mut BZip2Decoder::new())
            .collect::<Result<Vec<_>, _>>()?;
        let decoded: WritableEnvironment = match bincode::deserialize(&decompressed_data) {
            Ok(environment) => environment,
            Err(e) => {
                warn!("Unable to deserialize env: {} {:?}", key, e);
                return Err(e.into());
            }
        };
        Ok(Some(decoded.into()))
    } else {
        debug!("Unable to find {} in sqlite db", key);
        Ok(None)
    }
}

pub fn write_sql_env(db_path: &str, env: &str, sha: &str, data: &Environment) -> Result<usize> {
    let conn = open_sql_db(db_path, false)?;
    let key = gen_env_key(sha, env);
    let env_data: WritableEnvironment = data.into();
    let data = bincode::serialize(&env_data)?;
    let data_len = data.len();
    let compressed_data = data
        .into_iter()
        .encode(&mut BZip2Encoder::new(6), Action::Finish)
        .collect::<Result<Vec<_>, _>>()?;
    debug!(
        "Writing to DB. Key: {} Size: {} -> {} = {}",
        key,
        data_len,
        compressed_data.len(),
        data_len - compressed_data.len()
    );

    conn.execute(
        "INSERT INTO hogan (key, data) VALUES (?1, ?2)",
        params![key, compressed_data],
    )
    .map_err(|e| e.into())
}

pub fn gen_env_key(sha: &str, env: &str) -> String {
    format!("{}::{}", env, sha)
}

pub fn gen_env_listing_key(sha: &str) -> String {
    format!("!listing::{}", sha)
}

#[derive(Default, Serialize, Deserialize, Debug)]
struct WritableEnvironment {
    config_data: String,
    environment: String,
    environment_type: Option<String>,
}

impl From<&Environment> for WritableEnvironment {
    fn from(environment: &Environment) -> Self {
        WritableEnvironment {
            config_data: environment.config_data.to_string(),
            environment: environment.environment.to_owned(),
            environment_type: environment.environment_type.to_owned(),
        }
    }
}

impl From<WritableEnvironment> for Environment {
    fn from(environment: WritableEnvironment) -> Self {
        Environment {
            config_data: serde_json::from_str(&environment.config_data).unwrap(),
            environment: environment.environment.to_owned(),
            environment_type: environment.environment_type.to_owned(),
        }
    }
}

#[derive(Default, Serialize, Deserialize, Debug)]
struct WritableEnvironmentListing {
    environments: Vec<EnvironmentDescription>,
}

impl From<&[EnvironmentDescription]> for WritableEnvironmentListing {
    fn from(environments: &[EnvironmentDescription]) -> Self {
        Self {
            environments: environments.to_owned(),
        }
    }
}

pub fn write_sql_env_listing(
    db_path: &str,
    sha: &str,
    data: &[EnvironmentDescription],
) -> Result<usize> {
    let conn = open_sql_db(db_path, false)?;
    let key = gen_env_listing_key(sha);
    let env_data: WritableEnvironmentListing = data.into();
    let data = bincode::serialize(&env_data)?;
    let data_len = data.len();
    let compressed_data = data
        .into_iter()
        .encode(&mut BZip2Encoder::new(6), Action::Finish)
        .collect::<Result<Vec<_>, _>>()?;
    debug!(
        "Writing to DB. Key: {} Size: {} -> {} = {}",
        key,
        data_len,
        compressed_data.len(),
        data_len - compressed_data.len()
    );

    conn.execute(
        "INSERT INTO hogan (key, data) VALUES (?1, ?2)",
        params![key, compressed_data],
    )
    .map_err(|e| e.into())
}

pub fn read_sql_env_listing(
    db_path: &str,
    sha: &str,
) -> Result<Option<Vec<EnvironmentDescription>>> {
    let conn = open_sql_db(db_path, true)?;
    let mut query = conn.prepare("SELECT data FROM hogan WHERE key LIKE ? || '%' LIMIT 1")?;
    let key = gen_env_listing_key(sha);
    let data: Option<rusqlite::Result<Vec<u8>>> =
        query.query_map(params![key], |row| row.get(0))?.next();
    if let Some(data) = data {
        let decompressed_data = data?
            .into_iter()
            .decode(&mut BZip2Decoder::new())
            .collect::<Result<Vec<_>, _>>()?;
        let decoded: WritableEnvironmentListing = match bincode::deserialize(&decompressed_data) {
            Ok(environment) => environment,
            Err(e) => {
                warn!("Unable to deserialize env: {} {:?}", key, e);
                return Err(e.into());
            }
        };
        Ok(Some(decoded.environments))
    } else {
        debug!("Unable to find {} in sqlite db", key);
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteCleanup {}

#[actor(ExecuteCleanup)]
#[derive(Debug)]
struct CleanupActor {
    db_path: String,
    max_age: usize,
    metrics: Arc<DdMetrics>,
}

impl ActorFactoryArgs<(String, usize, Arc<DdMetrics>)> for CleanupActor {
    fn create_args((db_path, max_age, metrics): (String, usize, Arc<DdMetrics>)) -> Self {
        CleanupActor {
            db_path,
            max_age,
            metrics,
        }
    }
}

impl Actor for CleanupActor {
    type Msg = CleanupActorMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        self.receive(ctx, msg, sender);
    }
}

impl Receive<ExecuteCleanup> for CleanupActor {
    type Msg = CleanupActorMsg;

    fn receive(&mut self, _ctx: &Context<Self::Msg>, _msg: ExecuteCleanup, _sender: Sender) {
        let now = SystemTime::now();
        match clean_db(&self.db_path, self.max_age) {
            Ok(rows) => {
                let duration = now.elapsed().unwrap_or(Duration::from_millis(0));
                info!(
                    "Cleaned {} rows from the database older than {} days. Time {} ms",
                    rows,
                    self.max_age,
                    duration.as_millis()
                );
                self.metrics.time(
                    CustomMetrics::DbCleanup.into(),
                    None,
                    duration.as_millis() as i64,
                );
            }
            Err(err) => {
                error!("Unable to clean the db {:?}", err);
            }
        }
    }
}

pub fn init_db_cleanup_system(
    system: &ActorSystem,
    db_path: &str,
    max_age: usize,
    metrics: Arc<DdMetrics>,
) {
    let cleanup_poller_delay = 24 * 60 * 60; //1 day
    let worker = system
        .actor_of_args::<CleanupActor, _>(
            "db-cleanup-worker",
            (db_path.to_owned(), max_age, metrics),
        )
        .unwrap();

    system.schedule(
        Duration::from_secs(cleanup_poller_delay),
        Duration::from_secs(cleanup_poller_delay),
        worker.clone(),
        None,
        ExecuteCleanup {},
    );

    info!(
        "Scheduled db cleanup poller for every {} s",
        cleanup_poller_delay
    );

    worker.tell(ExecuteCleanup {}, None);
}
