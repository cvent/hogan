use crate::app::config::AppCommon;
use crate::app::datadogstatsd::{CustomMetrics, DdMetrics};
use crate::app::fetch_actor;
use crate::app::head_actor;
use crate::storage::cache::Cache;
use crate::storage::{lru, multi, sqlite};
use actix_web::dev::Service;
use actix_web::middleware::Logger;
use actix_web::{get, middleware, post, web, HttpResponse, HttpServer};
use anyhow::{Context, Result};
use hogan::config::{ConfigDir, EnvironmentDescription};
use hogan::error::HoganError;
use parking_lot::Mutex;
use regex::Regex;
use riker::actors::ActorSystem;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

struct ServerState {
    cache: Arc<multi::MultiCache>,
    config_dir: Arc<hogan::config::ConfigDir>,
    write_lock: Mutex<usize>,
    environments_regex: Regex,
    strict: bool,
    allow_fetch: bool,
    dd_metrics: Arc<DdMetrics>,
    environment_pattern: String,
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
        HoganError::InvalidTemplate { msg, env } => {
            let mut body = response_map();
            body.insert("message", &msg);
            body.insert("environment", &env);
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

#[allow(clippy::too_many_arguments)]
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

    let cache = Arc::new(multi::MultiCache::new(vec![
        Box::new(lru::LruEnvCache::new("lru", cache_size)?),
        Box::new(sqlite::SqliteCache::new(&db_path)),
    ]));

    let write_lock = Mutex::new(0);

    info!("Starting server on {}:{}", address, port);

    let state = ServerState {
        //environments,
        //environment_listings,
        cache,
        config_dir,
        write_lock,
        environments_regex,
        strict: common.strict,
        dd_metrics,
        environment_pattern,
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
    path.split('/').nth(1).unwrap_or("route")
}

#[actix_web::main]
async fn start_server(address: String, port: u16, state: ServerState) -> std::io::Result<()> {
    let binding = format!("{}:{}", address, port);
    let dd_client = state.dd_metrics.clone();
    let server_state = web::Data::new(state);

    HttpServer::new(move || {
        let dd_client = dd_client.clone();
        actix_web::App::new()
            .wrap_fn(move |req, srv| {
                let start_time = if req.path() != "/ok" {
                    Some(SystemTime::now())
                } else {
                    None
                };
                let dd_client = dd_client.clone();
                let fut = srv.call(req);

                async move {
                    let res = fut.await?;
                    if let Some(start) = start_time {
                        if let Ok(duration) = start.elapsed() {
                            let path = contextualize_path(res.request().path());
                            let method = res.request().method().as_str();
                            let ms = duration.as_millis();
                            let status = res.status();
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
                    };
                    Ok(res)
                }
            })
            .wrap(Logger::default())
            .wrap(middleware::Compress::default())
            .app_data(server_state.clone())
            .service(transform_route_sha_env)
            .service(transform_branch_head)
            .service(get_envs)
            .service(get_config_by_env)
            .service(get_config_by_env_branch)
            .service(get_branch_sha)
            .service(ok_route)
    })
    .bind(binding)?
    .run()
    .await
}

#[get("ok")]
async fn ok_route() -> HttpResponse {
    HttpResponse::Ok().finish()
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
    let result: Result<String> = match web::block(move || {
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

fn transform_from_sha(
    data: String,
    sha: &str,
    env_name: &str,
    state: &ServerState,
) -> Result<String> {
    let sha = format_sha(sha);

    let possible_envs = get_env_listing(state, None, sha)?;

    //This is a check to see if the environment being queried even exists in the sha. Since envs are already cached, this hit is better than
    //the cost of a full miss
    if !possible_envs
        .iter()
        .any(|env| env.environment_name == env_name)
    {
        return Err(HoganError::UnknownEnvironment {
            sha: sha.to_string(),
            env: env_name.to_string(),
        }
        .into());
    }

    let env = get_env(state, None, sha, env_name)?;

    let handlebars = hogan::transform::handlebars(state.strict);
    handlebars
        .render_template(&data, &env.config_data)
        .map_err(|e| {
            HoganError::InvalidTemplate {
                msg: format!("Template Error {:?}", e),
                env: env_name.to_string(),
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
    let result = match web::block(move || get_env_listing(&state, None, &params.sha)).await {
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

    let result = match web::block(move || get_env(&state, None, &sha, &params.env)).await {
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
    let result = match web::block(move || {
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
    let result = match web::block(move || find_branch_head(&params.branch_name, &state)).await {
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
    let result = match web::block(move || {
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

fn register_cache_hit(state: &ServerState) {
    state.dd_metrics.incr(
        CustomMetrics::Cache.into(),
        Some(vec!["action:hit".to_string()]),
    );
}

fn register_cache_miss(state: &ServerState) {
    state.dd_metrics.incr(
        CustomMetrics::Cache.into(),
        Some(vec!["action:miss".to_string()]),
    );
}

fn get_env_from_cache(
    state: &ServerState,
    env: &str,
    sha: &str,
) -> Option<Arc<hogan::config::Environment>> {
    //let cache = state.cache.lock();
    if let Ok(Some(env)) = state.cache.read_env(env, sha) {
        register_cache_hit(state);
        Some(env.clone())
    } else {
        None
    }
}

fn insert_into_env_cache(
    state: &ServerState,
    env: &str,
    sha: &str,
    data: hogan::config::Environment,
) -> Arc<hogan::config::Environment> {
    if let Err(e) = state.cache.write_env(env, sha, &data) {
        error!(
            "Issue writing environment to cache: {} {} {:?}",
            env, sha, e
        );
    };
    Arc::new(data)
}

fn get_env(
    state: &ServerState,
    remote: Option<&str>,
    sha: &str,
    env: &str,
) -> Result<Arc<hogan::config::Environment>> {
    if let Some(env) = get_env_from_cache(state, env, sha) {
        Ok(env)
    } else {
        let _write_lock = state.write_lock.lock();

        //Double check if the cache now contains the env we are looking for
        if let Some(environment) = get_env_from_cache(state, env, sha) {
            register_cache_hit(state);
            debug!("Avoided git lock for config lookup: {} {}", env, sha);
            return Ok(environment);
        }

        register_cache_miss(state);

        let full_sha = state
            .config_dir
            .refresh(remote, Some(sha), state.allow_fetch)?;

        let filter = match hogan::config::build_env_regex(env, Some(&state.environment_pattern)) {
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
            Ok(insert_into_env_cache(
                state,
                env,
                &full_sha,
                environment.clone(),
            ))
        } else {
            debug!("Unable to find the env {} in {}", env, full_sha);
            Err(HoganError::UnknownEnvironment {
                sha: full_sha,
                env: env.to_owned(),
            }
            .into())
        }
    }
}

fn check_env_listing_cache(
    state: &ServerState,
    sha: &str,
) -> Option<Arc<Vec<EnvironmentDescription>>> {
    let sha = format_sha(sha);
    if let Ok(Some(env)) = state.cache.read_env_listing(sha) {
        register_cache_hit(state);
        Some(env)
    } else {
        None
    }
}

fn insert_into_env_listing_cache(
    state: &ServerState,
    sha: &str,
    data: Vec<EnvironmentDescription>,
) -> Arc<Vec<EnvironmentDescription>> {
    let sha = format_sha(sha);
    let arc_data = Arc::new(data);
    if let Err(e) = state.cache.write_env_listing(sha, &arc_data) {
        error!("Error writing env listing to cache: {} {:?}", sha, e);
    };
    arc_data
}

fn get_env_listing(
    state: &ServerState,
    remote: Option<&str>,
    sha: &str,
) -> Result<Arc<Vec<EnvironmentDescription>>> {
    let sha = format_sha(sha);
    if let Some(env) = check_env_listing_cache(state, sha) {
        Ok(env)
    } else {
        let _write_lock = state.write_lock.lock();

        //Check if the cache has what we are looking for again
        if let Some(env) = check_env_listing_cache(state, sha) {
            return Ok(env);
        }

        register_cache_miss(state);
        let sha = state
            .config_dir
            .refresh(remote, Some(sha), state.allow_fetch)?;
        let envs = format_envs(&state.config_dir.find(state.environments_regex.clone()));

        Ok(insert_into_env_listing_cache(state, &sha, envs))
    }
}

fn format_envs(envs: &[hogan::config::Environment]) -> Vec<EnvironmentDescription> {
    envs.iter().map(|e| e.into()).collect()
}

fn format_sha(sha: &str) -> &str {
    if sha.len() >= 7 {
        &sha[0..7]
    } else {
        sha
    }
}
