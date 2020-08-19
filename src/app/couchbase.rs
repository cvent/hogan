use couchbase::*;
use futures::executor::block_on;
// use serde::Deserialize;
// use serde::Serialize;
use hogan::config::Environment;
use failure::Error;
use serde_json::json;
use crate::app::db;
use std::time::Duration;



pub struct CbConn {
    host: String,
    username: String,
    password: String,
    bucket: String,
    prefix: Option<String>
}
impl CbConn {
    pub fn format(conn_str: &str) -> CbConn {
        let vars: Vec<&str> = conn_str.split(';').collect();
        let mut host = "";
        let mut username = "";
        let mut password = "";
        let mut bucket = "";
        let mut prefix = None;
        for var in vars{
            let param: Vec<&str> = var.split('=').collect();
            let name = param[0];
            let val = param[1];
            match name {
                "Host" => host = val,
                "Bucket" => bucket = val,
                "Password" => password = val,
                "Username" => username = val,
                "Prefix" => prefix = Some(val.to_string()),
                &_ => ()

            }
        }
        CbConn {
            host: host.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            bucket: bucket.to_string(),
            prefix: prefix
        }       
    }

}



fn connect(cb_conn: &CbConn) ->  ( Collection, Cluster)  {
    // Connect to the cluster with a connection string and credentials
    let cluster =  Cluster::connect(&cb_conn.host, &cb_conn.username, &cb_conn.password);
    // Open a bucket
    let bucket = cluster.bucket(&cb_conn.bucket);
    // Use the default collection (needs to be used for all server 6.5 and earlier)
    return ( bucket.default_collection(), cluster ) 
}


pub fn read_cb_env(cb_conn: &CbConn, env: &str, sha: &str) -> Result<Option<Environment>, Error> {
    let collection = connect(cb_conn).0;
    let key = gen_cb_env_key(&cb_conn.prefix, sha, env);
    // Fetch a document

    info!("Fetching document from Couchbase DB. Key {}", key);
    match block_on(collection.get(key, GetOptions::default())) {
        Ok(r) => {
            // info!("get result: {:?}", r);
            let decoded: db::WritableEnvironment = match r.content(){
                Ok(environment) => environment,
                Err(e) => {
                    warn!("Unable to get couchbase document");
                    return Err(e.into());
                }
            };
            Ok(Some(decoded.into()))
        },
        Err(e) => {
            match e {
                CouchbaseError::DocumentNotFound {ctx: _} =>{
                    info!("couchbase document not found");
                    Ok(None)
                }
                _ => {
                    info!("get failed! {}", e);
                    Err(e.into())
                }            
            }
        }
    }   
}

pub fn write_cb_env(
    cb_conn: &CbConn,
    env: &str,
    sha: &str,
    data: &Environment,
) -> Result<Option<usize>, Error> {
    let collection = connect(cb_conn).0;
    let key = gen_cb_env_key(&cb_conn.prefix, sha, env);
    let env_data: db::WritableEnvironment = data.into();
    info!("Writing to Couchbase DB. Key {}", key);

    // Expiry set at 24hs
    let expiry = Duration::new(86400, 0);
    let upsert_options = UpsertOptions::default().expiry(expiry);

    match block_on(collection.upsert(key, env_data, upsert_options)) {
        Ok(r) => {
            info!("upsert result: {:?}", r);
            Ok(None)
        },
        Err(e) => {
            info!("upsert failed! {}", e);
            Err(e.into())
        },
    }
}

pub fn is_env_exist(cb_conn: &CbConn, env: &str, sha: &str) -> Result<Option<bool>, Error> {
    // let collection = connect(cb_conn);
    let cluster = connect(cb_conn).1;
    let key = gen_cb_env_key(&cb_conn.prefix, sha, env);


    info!("Checking document id from Couchbase DB. Key {}", key);

    // the exists api seems to get wrong result when document is expired or deleted
    // match block_on(collection.exists( key, ExistsOptions::default())) {
    //     Ok(result) => {
    //         println!("{:?}", result);
    //         Ok(Some(result.exists()))
    //     }
    //     Err(e) => {
    //         info!("query failed! {}", e);
    //         Err(e.into())
    //     },
    // }


    let options = QueryOptions::default().named_parameters(json!({"id": key}));
    let statement = format!("select meta().id from {} USE KEYS [$id]", &cb_conn.bucket);
    match block_on(cluster.query(
        statement,
        options,
    )) {
        Ok(mut result) => {
            match block_on(result.meta_data()).metrics().result_size() {
                0 => Ok(Some(false)),
                _ => Ok(Some(true))
            }
        }
        Err(e) => {
            info!("query failed! {}", e);
            Err(e.into())
        },
    }



}


fn gen_cb_env_key(prefix: &Option<String>, sha: &str, env: &str) -> String {
    match prefix {
        Some(prefix) => format!("{}::{}::{}", prefix, sha, env),
        None => format!("{}::{}", sha, env) 
    } 
}
