use crate::app::config::AppCommon;
use crate::app::datadogstatsd::{CustomMetrics, DdMetrics};
use crate::app::db;
use actix_service::Service;
use actix_web::{get, middleware, post, web, HttpResponse, HttpServer};
use failure::Error;
use futures::future::FutureExt;
use hogan;
use hogan::config::ConfigDir;
use lru_time_cache::LruCache;
use parking_lot::Mutex;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use std::time::SystemTime;

type EnvCache = Mutex<LruCache<String, Arc<hogan::config::Environment>>>;
type EnvListingCache = Mutex<LruCache<String, Arc<Vec<EnvDescription>>>>;

struct ServerState {
    environments: EnvCache,
    environment_listings: EnvListingCache,
    config_dir: Arc<hogan::config::ConfigDir>,
    write_lock: Mutex<usize>,
    environments_regex: Regex,
    strict: bool,
    dd_metrics: Option<DdMetrics>,
    environment_pattern: String,
    db_path: String,
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
) -> Result<(), Error> {
    let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;

    let environments =
        Mutex::new(LruCache::<String, Arc<hogan::config::Environment>>::with_capacity(cache_size));

    let environment_listings = Mutex::new(
        LruCache::<String, Arc<Vec<EnvDescription>>>::with_capacity(cache_size),
    );

    let config_dir = Arc::new(config_dir);
    let write_lock = Mutex::new(0);

    info!("Starting server on {}:{}", address, port);
    info!("datadog monitoring is setting: {}", datadog);
    let dd_metrics = if datadog {
        Some(DdMetrics::new())
    } else {
        None
    };

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
    };
    start_server(address, port, state, datadog)?;

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

#[actix_rt::main]
async fn start_server(
    address: String,
    port: u16,
    state: ServerState,
    dd_enabled: bool,
) -> Result<(), Error> {
    let binding = format!("{}:{}", address, port);
    let server_state = web::Data::new(state);

    HttpServer::new(move || {
        actix_web::App::new()
            .wrap(middleware::Compress::default())
            .app_data(server_state.clone())
            .wrap_fn(move |req, srv| {
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
                                if dd_enabled {
                                    let metrics = DdMetrics::new();
                                    metrics.time(
                                        CustomMetrics::RequestTime.metrics_name(),
                                        Some(vec![
                                            format!("url:{}", path),
                                            format!("method:{}", method),
                                            format!("status:{}", status.as_str()),
                                        ]),
                                        ms as i64,
                                    );
                                };
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

#[derive(Deserialize)]
struct TransformEnvParams {
    sha: String,
    env: String,
}

lazy_static! {
    static ref HEX_REGEX: Regex = Regex::new(r"^[a-f0-9]+$").unwrap();
}

#[post("transform/{sha}/{env}")]
fn transform_route_sha_env(
    data: String,
    params: web::Path<TransformEnvParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    //We keep running into folks that are passing in branch name here and it throws off the caching layer and gives inconsistent results
    //This won't catch branch names with all hex values, but would catch the common case like 'master'
    let sha = if !HEX_REGEX.is_match(&params.sha) {
        if let Some(head_sha) = find_branch_head(&params.sha, &state) {
            head_sha
        } else {
            return HttpResponse::NotFound().finish();
        }
    } else {
        params.sha.to_owned()
    };

    match transform_from_sha(data, &sha, &params.env, &state) {
        Ok(result) => HttpResponse::Ok().body(result),
        Err(e) => {
            warn!("Error templating request {} {} {}", e, sha, params.env);
            HttpResponse::BadRequest().finish()
        }
    }
}

fn transform_from_sha(
    data: String,
    sha: &str,
    env: &str,
    state: &ServerState,
) -> Result<String, Error> {
    let sha = format_sha(sha);
    match get_env(&state, None, sha, env) {
        Some(env) => {
            let handlebars = hogan::transform::handlebars(state.strict);
            handlebars
                .render_template(&data, &env.config_data)
                .map_err(|e| e.into())
        }
        None => Err(format_err!("Could not find env {}", env)),
    }
}

#[derive(Deserialize)]
struct GetEnvsParams {
    sha: String,
}

#[get("envs/{sha}")]
fn get_envs(params: web::Path<GetEnvsParams>, state: web::Data<ServerState>) -> HttpResponse {
    match get_env_listing(&state, None, &params.sha) {
        Some(envs) => HttpResponse::Ok().json(envs),
        None => HttpResponse::NotFound().finish(),
    }
}

#[derive(Deserialize)]
struct ConfigByEnvState {
    sha: String,
    env: String,
}

#[get("configs/{sha}/{env}")]
fn get_config_by_env(
    params: web::Path<ConfigByEnvState>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let sha = format_sha(&params.sha);
    match get_env(&state, None, sha, &params.env) {
        Some(env) => HttpResponse::Ok().json(env),
        None => HttpResponse::NotFound().finish(),
    }
}

#[derive(Deserialize)]
struct ConfigByEnvBranchState {
    branch_name: String,
    env: String,
}

#[get("branch/{branch_name:.*}/configs/{env}")]
fn get_config_by_env_branch(
    params: web::Path<ConfigByEnvBranchState>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    if let Some(head_sha) = find_branch_head(&params.branch_name, &state) {
        let sha = format_sha(&head_sha);
        match get_env(&state, None, sha, &params.env) {
            Some(env) => HttpResponse::Ok().json(env),
            None => HttpResponse::NotFound().finish(),
        }
    } else {
        HttpResponse::NotFound().finish()
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

fn find_branch_head(branch_name: &str, state: &ServerState) -> Option<String> {
    if let Some(head_sha) = state.config_dir.find_branch_head(&"origin", branch_name) {
        Some(head_sha)
    } else {
        None
    }
}

#[get("heads/{branch_name:.*}")]
fn get_branch_sha(
    params: web::Path<BranchShaParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let branch_name = &params.branch_name;
    debug!("Looking up branch name {}", branch_name);

    if let Some(head_sha) = find_branch_head(branch_name, &state) {
        HttpResponse::Ok().json(ShaResponse {
            head_sha,
            branch_name: branch_name.to_string(),
        })
    } else {
        HttpResponse::NotFound().finish()
    }
}

#[derive(Deserialize)]
struct BranchHeadTransformParams {
    branch_name: String,
    environment: String,
}

#[post("branch/{branch_name:.*}/transform/{environment}")]
fn transform_branch_head(
    data: String,
    params: web::Path<BranchHeadTransformParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    if let Some(head_sha) = find_branch_head(&params.branch_name, &state) {
        match transform_from_sha(data, &head_sha, &params.environment, &state) {
            Ok(result) => HttpResponse::Ok().body(result),
            Err(e) => {
                warn!(
                    "Error templating request {} {} {}",
                    e, head_sha, params.environment
                );
                HttpResponse::BadRequest().body(format!(
                    "Unable to template request on branch {} for environment {} - {:?}",
                    params.branch_name, params.environment, e
                ))
            }
        }
    } else {
        HttpResponse::NotFound().body(format!("Unknown branch {}", params.branch_name))
    }
}

fn format_key(sha: &str, env: &str) -> String {
    format!("{}::{}", sha, env)
}

fn register_cache_hit(state: &ServerState, key: &str) {
    info!("Cache Hit {}", key);
    if let Some(custom_metrics) = &state.dd_metrics {
        custom_metrics.incr(CustomMetrics::CacheHit.metrics_name(), None);
    }
}

fn register_cache_miss(state: &ServerState, key: &str) {
    info!("Cache Miss {}", key);
    if let Some(custom_metrics) = &state.dd_metrics {
        custom_metrics.incr(CustomMetrics::CacheMiss.metrics_name(), None);
    }
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
) -> Option<Arc<hogan::config::Environment>> {
    let key = format_key(sha, env);

    if let Some(env) = get_env_from_cache(state, &key) {
        Some(env)
    } else {
        //Check embedded db before git repo
        if let Some(environment) = db::read_sql_env(&state.db_path, env, sha).unwrap_or(None) {
            info!("Found environment in the db {} {}", env, sha);
            Some(insert_into_env_cache(state, &key, environment))
        } else {
            let _write_lock = state.write_lock.lock();

            //Double check if the cache now contains the env we are looking for
            if let Some(environment) = db::read_sql_env(&state.db_path, env, sha).unwrap_or(None) {
                register_cache_hit(state, &key);
                info!("Avoided git lock for config lookup: {}", key);
                return Some(Arc::new(environment));
            }

            register_cache_miss(state, &key);
            if let Some(sha) = state.config_dir.refresh(remote, Some(sha)) {
                let filter =
                    match hogan::config::build_env_regex(env, Some(&state.environment_pattern)) {
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
                    Some(insert_into_env_cache(state, &key, environment.clone()))
                } else {
                    debug!("Unable to find the env {} in {}", env, sha);
                    None
                }
            } else {
                debug!("Unable to find the sha {}", sha);
                None
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
) -> Option<Arc<Vec<EnvDescription>>> {
    let sha = format_sha(sha);
    if let Some(env) = check_env_listing_cache(state, &sha) {
        Some(env)
    } else {
        {
            let _write_lock = state.write_lock.lock();

            //Check if the cache has what we are looking for again
            if let Some(env) = check_env_listing_cache(state, &sha) {
                return Some(env);
            }

            register_cache_miss(state, sha);
            if let Some(sha) = state.config_dir.refresh(remote, Some(sha)) {
                let envs = format_envs(&state.config_dir.find(state.environments_regex.clone()));
                if !envs.is_empty() {
                    info!("Loading envs for {}", sha);
                    Some(insert_into_env_listing_cache(state, &sha, envs))
                } else {
                    info!("No envs found for {}", sha);
                    None
                }
            } else {
                info!("Fetch env listings. Unknown SHA: {}", sha);
                None
            }
        }
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
