use crate::app::config::AppCommon;
use crate::app::datadogstatsd::{CustomMetrics, DdMetrics};
use crate::app::db;
use crate::app::fetch_actor;
use crate::app::head_actor;
use actix_service::Service;
use actix_web::middleware::Logger;
use actix_web::{get, middleware, post, web, HttpResponse, HttpServer};
use anyhow::{Context, Result};
use futures::future::FutureExt;
use hogan::config::ConfigDir;
use hogan::error::HoganError;
use lru_time_cache::LruCache;
use parking_lot::Mutex;
use regex::Regex;
use riker::actors::ActorSystem;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::task;

type EnvCache = Mutex<LruCache<String, Arc<hogan::config::Environment>>>;
type EnvListingCache = Mutex<LruCache<String, Arc<Vec<EnvDescription>>>>;

struct ServerState {
    environments: EnvCache,
    environment_listings: EnvListingCache,
    config_dir: Arc<hogan::config::ConfigDir>,
    write_lock: Mutex<usize>,
    environments_regex: Regex,
    strict: bool,
    allow_fetch: bool,
    dd_metrics: Arc<DdMetrics>,
    environment_pattern: String,
    db_path: String,
    actor_system: ActorSystem,
    head_request_actor: head_actor::HeadRequestActor,
}

fn response_map<'a>() -> HashMap<&'a str, &'a str> {
    HashMap::new()
}

fn create_error_response(e: anyhow::Error) -> HttpResponse {
    debug!("An error occurred: {:?}", e);
    let he = e.into();
    match he {
        HoganError::BadRequest => HttpResponse::BadGateway().finish(),
        HoganError::GitError { msg } => {
            let mut body = response_map();
            body.insert("message", &msg);
            HttpResponse::InternalServerError().json(body)
        }
        HoganError::UnknownBranch { branch } => {
            let mut body = response_map();
            body.insert("branch", &branch);
            body.insert("message", "Unknown branch");
            HttpResponse::NotFound().json(body)
        }
        HoganError::UnknownSHA { sha } => {
            let mut body = response_map();
            body.insert("sha", &sha);
            body.insert("message", "Unknown sha");
            HttpResponse::NotFound().json(body)
        }
        HoganError::InvalidTemplate { msg } => {
            let mut body = response_map();
            body.insert("message", &msg);
            HttpResponse::BadRequest().json(body)
        }
        HoganError::UnknownEnvironment { sha, env } => {
            let mut body = response_map();
            body.insert("sha", &sha);
            body.insert("environment", &env);
            body.insert("message", "Unknown Environment");
            HttpResponse::NotFound().json(body)
        }
        HoganError::InternalTimeout => {
            error!("Internal Timeout Occurred {:?}", he);
            HttpResponse::ServiceUnavailable().finish()
        }
        _ => {
            error!("An unexpected error occurred {:?}", he);
            HttpResponse::InternalServerError().finish()
        }
    }
}

pub fn start_up_server(
    common: AppCommon,
    port: u16,
    address: String,
    cache_size: usize,
    environments_regex: Regex,
    datadog: bool,
    environment_pattern: String,
    db_path: String,
    fetch_poller: u64,
    allow_fetch: bool,
) -> Result<()> {
    info!("datadog monitoring is setting: {}", datadog);
    let dd_metrics = Arc::new(DdMetrics::new(datadog));
    let config_dir = Arc::new(ConfigDir::new(
        common.configs_url,
        &common.ssh_key,
        common.native_git,
        common.native_fetch,
        common.native_clone,
    )?);

    let actor_system = ActorSystem::new()?;
    let head_request_actor =
        head_actor::init_system(&actor_system, config_dir.clone(), allow_fetch);

    fetch_actor::init_system(
        &actor_system,
        config_dir.clone(),
        dd_metrics.clone(),
        fetch_poller,
    );

    let environments =
        Mutex::new(LruCache::<String, Arc<hogan::config::Environment>>::with_capacity(cache_size));

    let environment_listings = Mutex::new(
        LruCache::<String, Arc<Vec<EnvDescription>>>::with_capacity(cache_size),
    );

    let write_lock = Mutex::new(0);

    info!("Starting server on {}:{}", address, port);

    let state = ServerState {
        environments,
        environment_listings,
        config_dir,
        write_lock,
        environments_regex,
        strict: common.strict,
        dd_metrics,
        environment_pattern,
        db_path,
        actor_system,
        head_request_actor,
        allow_fetch,
    };
    start_server(address, port, state)?;

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct EnvDescription {
    name: String,
    #[serde(rename(serialize = "Type"))]
    env_type: Option<String>,
}

impl From<&hogan::config::Environment> for EnvDescription {
    fn from(env: &hogan::config::Environment) -> EnvDescription {
        EnvDescription {
            name: env.environment.clone(),
            env_type: env.environment_type.clone(),
        }
    }
}

fn contextualize_path(path: &str) -> &str {
    path.split('/').nth(1).unwrap_or_else(|| &"route")
}

#[actix_web::main]
async fn start_server(address: String, port: u16, state: ServerState) -> Result<()> {
    let binding = format!("{}:{}", address, port);
    let dd_client = state.dd_metrics.clone();
    let server_state = web::Data::new(state);

    HttpServer::new(move || {
        let dd_client = dd_client.clone();
        actix_web::App::new()
            .wrap(Logger::default())
            .wrap(middleware::Compress::default())
            .app_data(server_state.clone())
            .wrap_fn(move |req, srv| {
                let dd_client = dd_client.clone();
                let start_time = if req.path() != "/ok" {
                    Some(SystemTime::now())
                } else {
                    None
                };
                srv.call(req).map(move |res| {
                    if let Ok(result) = res {
                        if let Some(time) = start_time {
                            if let Ok(duration) = time.elapsed() {
                                let path = contextualize_path(result.request().path());
                                let method = result.request().method().as_str();
                                let ms = duration.as_millis();
                                let status = result.status();
                                debug!(
                                    "Request for {} {} duration: {} status: {}",
                                    method, path, ms, status
                                );

                                dd_client.time(
                                    CustomMetrics::RequestTime.into(),
                                    Some(vec![
                                        format!("url:{}", path),
                                        format!("method:{}", method),
                                        format!("status:{}", status.as_str()),
                                    ]),
                                    ms as i64,
                                );
                            }
                        }
                        Ok(result)
                    } else {
                        res
                    }
                })
            })
            .service(transform_route_sha_env)
            .service(transform_branch_head)
            .service(get_envs)
            .service(get_config_by_env)
            .service(get_config_by_env_branch)
            .service(get_branch_sha)
            .route("/ok", web::to(|| HttpResponse::Ok().finish()))
    })
    .bind(binding)?
    .run()
    .await?;

    Ok(())
}

#[derive(Deserialize, Clone)]
struct TransformEnvParams {
    sha: String,
    env: String,
}

lazy_static! {
    static ref HEX_REGEX: Regex = Regex::new(r"^[a-f0-9]+$").unwrap();
}

#[post("transform/{sha}/{env}")]
async fn transform_route_sha_env(
    data: String,
    params: web::Path<TransformEnvParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let sha = params.sha.to_owned();
    let env = params.env.to_owned();
    let result: Result<String> = match task::spawn_blocking(move || {
        //We keep running into folks that are passing in branch name here and it throws off the caching layer and gives inconsistent results
        //This won't catch branch names with all hex values, but would catch the common case like 'master'
        let sha = if !HEX_REGEX.is_match(&sha) {
            match find_branch_head(&sha, &state) {
                Ok(sha) => sha,
                Err(e) => return Err(e),
            }
        } else {
            sha
        };

        transform_from_sha(data, &sha, &env, &state)
    })
    .await
    {
        Ok(r) => r,
        Err(e) => Err(e.into()),
    }
    .with_context(|| "Branch based transform");

    match result {
        Ok(body) => HttpResponse::Ok().body(body),
        Err(e) => create_error_response(e),
    }
}

fn transform_from_sha(data: String, sha: &str, env: &str, state: &ServerState) -> Result<String> {
    let sha = format_sha(sha);

    let env = get_env(&state, None, sha, env)?;

    let handlebars = hogan::transform::handlebars(state.strict);
    handlebars
        .render_template(&data, &env.config_data)
        .map_err(|e| {
            HoganError::InvalidTemplate {
                msg: format!("Template Error {:?}", e),
            }
            .into()
        })
}

#[derive(Deserialize)]
struct GetEnvsParams {
    sha: String,
}

#[get("envs/{sha}")]
async fn get_envs(params: web::Path<GetEnvsParams>, state: web::Data<ServerState>) -> HttpResponse {
    let result =
        match task::spawn_blocking(move || get_env_listing(&state, None, &params.sha)).await {
            Ok(envs) => envs,
            Err(e) => {
                warn!("Error joining on getting environments {:?}", e);
                Err(e.into())
            }
        };

    match result {
        Ok(envs) => HttpResponse::Ok().json(envs),
        Err(e) => create_error_response(e),
    }
}

#[derive(Deserialize)]
struct ConfigByEnvState {
    sha: String,
    env: String,
}

#[get("configs/{sha}/{env}")]
async fn get_config_by_env(
    params: web::Path<ConfigByEnvState>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let sha = format_sha(&params.sha).to_owned();

    let result = match task::spawn_blocking(move || get_env(&state, None, &sha, &params.env)).await
    {
        Ok(env) => env,
        Err(e) => {
            warn!("Error joining on getting environments {:?}", e);
            Err(e.into())
        }
    };

    match result {
        Ok(env) => HttpResponse::Ok().json(env),
        Err(e) => create_error_response(e),
    }
}

#[derive(Deserialize, Clone)]
struct ConfigByEnvBranchState {
    branch_name: String,
    env: String,
}

#[get("branch/{branch_name:.*}/configs/{env}")]
async fn get_config_by_env_branch(
    params: web::Path<ConfigByEnvBranchState>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let branch = params.branch_name.to_owned();
    let env = params.env.to_owned();
    let result = match task::spawn_blocking(move || {
        let head_sha = match find_branch_head(&branch, &state) {
            Ok(head_sha) => head_sha,
            Err(e) => return Err(e),
        };
        let sha = format_sha(&head_sha);

        get_env(&state, None, sha, &env)
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Error joining when querying configs by branch");
            Err(e.into())
        }
    };

    match result {
        Ok(result) => HttpResponse::Ok().json(result),
        Err(e) => create_error_response(e),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShaResponse {
    head_sha: String,
    branch_name: String,
}

#[derive(Deserialize)]
struct BranchShaParams {
    branch_name: String,
}

fn find_branch_head(branch_name: &str, state: &ServerState) -> Result<String> {
    head_actor::request_branch_head(&state.actor_system, &state.head_request_actor, branch_name)
}

#[get("heads/{branch_name:.*}")]
async fn get_branch_sha(
    params: web::Path<BranchShaParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let branch_name = params.branch_name.to_owned();
    debug!("Looking up branch name {}", branch_name);
    let result =
        match task::spawn_blocking(move || find_branch_head(&params.branch_name, &state)).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Error joining from branch sha {} {:?}", branch_name, e);
                Err(e.into())
            }
        };

    match result {
        Ok(head_sha) => HttpResponse::Ok().json(ShaResponse {
            head_sha,
            branch_name,
        }),
        Err(e) => create_error_response(e),
    }
}

#[derive(Deserialize)]
struct BranchHeadTransformParams {
    branch_name: String,
    environment: String,
}

#[post("branch/{branch_name:.*}/transform/{environment}")]
async fn transform_branch_head(
    data: String,
    params: web::Path<BranchHeadTransformParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    //Pull out values for later use
    let branch_name = params.branch_name.to_owned();
    let environment = params.environment.to_owned();
    //Double wrapped Option representing BRANCH(ENVIRONMENT(TEMPLATE))
    let result = match task::spawn_blocking(move || {
        let head_sha = match find_branch_head(&params.branch_name, &state) {
            Ok(sha) => sha,
            Err(e) => return Err(e),
        };

        transform_from_sha(data, &head_sha, &params.environment, &state)
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "Error joining from a template request {} {} {:?}",
                environment, branch_name, e
            );
            Err(e.into())
        }
    };

    match result {
        Ok(result) => HttpResponse::Ok().body(result),
        Err(e) => create_error_response(e),
    }
}

fn format_key(sha: &str, env: &str) -> String {
    format!("{}::{}", sha, env)
}

fn register_cache_hit(state: &ServerState, key: &str) {
    debug!("Cache Hit {}", key);
    state.dd_metrics.incr(
        CustomMetrics::Cache.into(),
        Some(vec!["action:hit".to_string()]),
    );
}

fn register_cache_miss(state: &ServerState, key: &str) {
    debug!("Cache Miss {}", key);
    state.dd_metrics.incr(
        CustomMetrics::Cache.into(),
        Some(vec!["action:miss".to_string()]),
    );
}

fn get_env_from_cache(state: &ServerState, key: &str) -> Option<Arc<hogan::config::Environment>> {
    let mut cache = state.environments.lock();
    if let Some(env) = cache.get(key) {
        register_cache_hit(state, key);
        Some(env.clone())
    } else {
        None
    }
}

fn insert_into_env_cache(
    state: &ServerState,
    key: &str,
    data: hogan::config::Environment,
) -> Arc<hogan::config::Environment> {
    let mut cache = state.environments.lock();
    let arc_data = Arc::new(data);
    cache.insert(key.to_owned(), arc_data.clone());
    arc_data
}

fn get_env(
    state: &ServerState,
    remote: Option<&str>,
    sha: &str,
    env: &str,
) -> Result<Arc<hogan::config::Environment>> {
    let key = format_key(sha, env);

    if let Some(env) = get_env_from_cache(state, &key) {
        Ok(env)
    } else {
        //Check embedded db before git repo
        if let Some(environment) = db::read_sql_env(&state.db_path, env, sha).unwrap_or(None) {
            debug!("Found environment in the db {} {}", env, sha);
            Ok(insert_into_env_cache(state, &key, environment))
        } else {
            let _write_lock = state.write_lock.lock();

            //Double check if the cache now contains the env we are looking for
            if let Some(environment) = db::read_sql_env(&state.db_path, env, sha).unwrap_or(None) {
                register_cache_hit(state, &key);
                debug!("Avoided git lock for config lookup: {}", key);
                return Ok(Arc::new(environment));
            }

            register_cache_miss(state, &key);

            let sha = state
                .config_dir
                .refresh(remote, Some(sha), state.allow_fetch)?;

            let filter = match hogan::config::build_env_regex(env, Some(&state.environment_pattern))
            {
                Ok(filter) => filter,
                Err(e) => {
                    warn!("Incompatible env name: {} {:?}", env, e);
                    //In an error scenario we'll still try and match against all configs
                    state.environments_regex.clone()
                }
            };
            if let Some(environment) = state
                .config_dir
                .find(filter)
                .iter()
                .find(|e| e.environment == env)
            {
                if let Err(e) = db::write_sql_env(&state.db_path, env, &sha, environment) {
                    warn!("Unable to write env {} {}::{} to db {:?}", key, sha, env, e);
                };
                Ok(insert_into_env_cache(state, &key, environment.clone()))
            } else {
                debug!("Unable to find the env {} in {}", env, sha);
                Err(HoganError::UnknownEnvironment {
                    sha,
                    env: env.to_owned(),
                }
                .into())
            }
        }
    }
}

fn check_env_listing_cache(state: &ServerState, sha: &str) -> Option<Arc<Vec<EnvDescription>>> {
    let sha = format_sha(sha);
    let mut cache = state.environment_listings.lock();
    if let Some(env) = cache.get(sha) {
        register_cache_hit(state, sha);
        Some(env.clone())
    } else {
        None
    }
}

fn insert_into_env_listing_cache(
    state: &ServerState,
    sha: &str,
    data: Vec<EnvDescription>,
) -> Arc<Vec<EnvDescription>> {
    let sha = format_sha(sha);
    let mut cache = state.environment_listings.lock();
    let arc_data = Arc::new(data);
    cache.insert(sha.to_owned(), arc_data.clone());
    arc_data
}

fn get_env_listing(
    state: &ServerState,
    remote: Option<&str>,
    sha: &str,
) -> Result<Arc<Vec<EnvDescription>>> {
    let sha = format_sha(sha);
    if let Some(env) = check_env_listing_cache(state, &sha) {
        Ok(env)
    } else {
        let _write_lock = state.write_lock.lock();

        //Check if the cache has what we are looking for again
        if let Some(env) = check_env_listing_cache(state, &sha) {
            return Ok(env);
        }

        register_cache_miss(state, sha);
        let sha = state
            .config_dir
            .refresh(remote, Some(sha), state.allow_fetch)?;
        let envs = format_envs(&state.config_dir.find(state.environments_regex.clone()));

        Ok(insert_into_env_listing_cache(state, &sha, envs))
    }
}

fn format_envs(envs: &[hogan::config::Environment]) -> Vec<EnvDescription> {
    envs.iter().map(|e| e.into()).collect()
}

fn format_sha(sha: &str) -> &str {
    if sha.len() >= 7 {
        &sha[0..7]
    } else {
        sha
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};

    #[actix_rt::test]
    async fn test_ok_route() {
        let mut app =
            test::init_service(App::new().route("/ok", web::to(|| HttpResponse::Ok().finish())))
                .await;
        let req = test::TestRequest::get().uri("/ok").to_request();
        let resp = test::call_service(&mut app, req).await;

        assert!(resp.status().is_success());
    }
}
