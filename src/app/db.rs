use bincode;
use failure::Error;
use hogan::config::Environment;
use serde::Deserialize;
use serde::Serialize;
use sled;
pub use sled::Db;
use tempfile;

pub fn open_db(db_name: Option<String>) -> Result<sled::Db, Error> {
    let path = match db_name {
        Some(p) => p,
        None => tempfile::tempdir()?
            .into_path()
            .to_str()
            .unwrap()
            .to_owned(),
    };
    info!("Opening DB: {}", path);
    sled::open(path).map_err(|e| e.into())
}

fn gen_key(sha: &str, env: &str) -> String {
    format!("{}::{}", sha, env)
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

pub fn write_env(db: &sled::Db, env: &str, sha: &str, data: &Environment) -> Result<(), Error> {
    write_key(db, &gen_key(sha, env), data)
}

fn write_key(db: &sled::Db, key: &str, data: &Environment) -> Result<(), Error> {
    let env: WritableEnvironment = data.into();
    let data: Vec<u8> = bincode::serialize(&env).unwrap();
    info!("Writing {} to db size: {}", key, data.len());
    db.insert(key, data)?;
    Ok(())
}

pub fn read_env(db: &sled::Db, env: &str, sha: &str) -> Result<Option<Environment>, Error> {
    let key = gen_key(sha, env);
    match db.get(&key)? {
        Some(data) => {
            let decoded: WritableEnvironment = match bincode::deserialize(&data) {
                Ok(environment) => environment,
                Err(e) => {
                    warn!("Unable to deserialize env: {} {:?}", key, e);
                    return Err(e.into());
                }
            };
            Ok(Some(decoded.into()))
        }
        None => Ok(None),
    }
}

pub fn remove_env(db: &sled::Db, env: &str, sha: &str) -> Result<(), Error> {
    db.remove(gen_key(sha, env))?;
    Ok(())
}
