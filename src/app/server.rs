use crate::app::config::AppCommon;
use crate::app::datadogstatsd::{CustomMetrics, DdMetrics};
use actix_service::Service;
use actix_web::{get, post, web, HttpResponse, HttpServer};
use failure::Error;
use futures::future::FutureExt;
use hogan;
use hogan::config::ConfigDir;
use lru_time_cache::LruCache;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;

pub fn start_up_server(
    common: AppCommon,
    port: u16,
    address: String,
    cache_size: usize,
    environments_regex: Regex,
    datadog: bool,
) -> Result<(), Error> {
    let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;

    let environments =
        Mutex::new(LruCache::<String, Arc<hogan::config::Environment>>::with_capacity(cache_size));

    let environment_listings = Mutex::new(
        LruCache::<String, Arc<Vec<EnvDescription>>>::with_capacity(cache_size),
    );

    let config_dir = Mutex::new(config_dir);

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
        environments_regex,
        strict: common.strict,
        dd_metrics,
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

type EnvCache = Mutex<LruCache<String, Arc<hogan::config::Environment>>>;
type EnvListingCache = Mutex<LruCache<String, Arc<Vec<EnvDescription>>>>;

struct ServerState {
    environments: EnvCache,
    environment_listings: EnvListingCache,
    config_dir: Mutex<hogan::config::ConfigDir>,
    environments_regex: Regex,
    strict: bool,
    dd_metrics: Option<DdMetrics>,
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
            .app_data(server_state.clone())
            .wrap_fn(move |req, srv| {
                let start_time = if req.path() != "/ok" {
                    Some(SystemTime::now())
                } else {
                    None
                };
                srv.call(req).map(move |res| {
                    if let Some(time) = start_time {
                        if let Ok(duration) = time.elapsed() {
                            let ms = duration.as_millis();
                            debug!("Request duration: {}", ms);
                            if dd_enabled {
                                let metrics = DdMetrics::new();
                                metrics.time(
                                    CustomMetrics::RequestTime.metrics_name(),
                                    "route", //TODO: Normalize the matched URI
                                    ms as i64,
                                );
                            };
                        }
                    }
                    res
                })
            })
            .service(
                web::scope("/transform")
                    .service(transform_env)
                    .service(transform_all_envs),
            )
            .service(web::scope("/envs").service(get_envs))
            .service(web::scope("/configs").service(get_config_by_env))
            .service(web::scope("/heads").service(get_branch_sha))
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

#[post("/{sha}/{env}")]
fn transform_env(
    data: String,
    params: web::Path<TransformEnvParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let sha = format_sha(&params.sha);
    let uri = format!("/transform/{}/{}", &sha, &params.env);
    match get_env(&state, None, sha, &params.env, &uri) {
        Some(env) => {
            let handlebars = hogan::transform::handlebars(state.strict);
            match handlebars.render_template(&data, &env.config_data) {
                Ok(result) => HttpResponse::Ok().body(result),
                Err(e) => {
                    warn!("Error templating request {} {} {}", e, sha, params.env);
                    HttpResponse::BadRequest().finish()
                }
            }
        }
        None => HttpResponse::NotFound().finish(),
    }
}

#[post("/{sha}?{filename}")]
fn transform_all_envs() -> HttpResponse {
    HttpResponse::Gone().finish()
}

#[derive(Deserialize)]
struct GetEnvsParams {
    sha: String,
}

#[get("/{sha}")]
fn get_envs(params: web::Path<GetEnvsParams>, state: web::Data<ServerState>) -> HttpResponse {
    let uri = format!("/envs/{}", &params.sha);
    info!("uri: {}", uri);

    match get_env_listing(&state, None, &params.sha, &uri) {
        Some(envs) => HttpResponse::Ok().json(envs),
        None => HttpResponse::NotFound().finish(),
    }
}

#[derive(Deserialize)]
struct ConfigByEnvState {
    sha: String,
    env: String,
}

#[get("/{sha}/{env}")]
fn get_config_by_env(
    params: web::Path<ConfigByEnvState>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    let sha = format_sha(&params.sha);
    let uri = format!("/config/{}/{}", &sha, &params.env);
    match get_env(&state, None, sha, &params.env, &uri) {
        Some(env) => HttpResponse::Ok().json(env),
        None => HttpResponse::NotFound().finish(),
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

#[get("/{branch_name:.*}")]
fn get_branch_sha(
    params: web::Path<BranchShaParams>,
    state: web::Data<ServerState>,
) -> HttpResponse {
    //let branch_name = branch_name.intersperse("/").collect::<String>();
    let branch_name = &params.branch_name;
    debug!("Looking up branch name {}", branch_name);
    if let Ok(config_dir) = state.config_dir.lock() {
        if let Some(head_sha) = config_dir.find_branch_head(&"origin", branch_name) {
            HttpResponse::Ok().json(ShaResponse {
                head_sha,
                branch_name: branch_name.to_string(),
            })
        } else {
            HttpResponse::NotFound().finish()
        }
    } else {
        warn!("Error locking git repo");
        HttpResponse::InternalServerError().finish()
    }
}

fn format_key(sha: &str, env: &str) -> String {
    format!("{}::{}", sha, env)
}

fn get_env(
    state: &ServerState,
    remote: Option<&str>,
    sha: &str,
    env: &str,
    request_url: &str,
) -> Option<Arc<hogan::config::Environment>> {
    let key = format_key(sha, env);
    let mut cache = match state.environments.lock() {
        Ok(cache) => cache,
        Err(e) => {
            warn!("Unable to lock cache {}", e);
            return None;
        }
    };
    if let Some(env) = cache.get(&key) {
        info!("Cache Hit {}", key);
        if let Some(custom_metrics) = &state.dd_metrics {
            custom_metrics.incr(CustomMetrics::CacheHit.metrics_name(), request_url);
        }
        Some(env.clone())
    } else {
        info!("Cache Miss {}", key);
        if let Some(custom_metrics) = &state.dd_metrics {
            custom_metrics.incr(CustomMetrics::CacheMiss.metrics_name(), request_url);
        }
        match state.config_dir.lock() {
            Ok(repo) => {
                if let Some(sha) = repo.refresh(remote, Some(sha)) {
                    match repo
                        .find(state.environments_regex.clone())
                        .iter()
                        .find(|e| e.environment == env)
                    {
                        Some(env) => cache.insert(key.clone(), Arc::new(env.clone())),
                        None => {
                            debug!("Unable to find the env {} in {}", env, sha);
                            return None;
                        }
                    };
                };
            }
            Err(e) => {
                warn!("Unable to lock repository {}", e);
                return None;
            }
        };
        if let Some(envs) = cache.get(&key) {
            Some(envs.clone())
        } else {
            info!("Unable to find the configuration sha {}", sha);
            None
        }
    }
}

fn get_env_listing(
    state: &ServerState,
    remote: Option<&str>,
    sha: &str,
    request_url: &str,
) -> Option<Arc<Vec<EnvDescription>>> {
    let sha = format_sha(sha);
    let mut cache = match state.environment_listings.lock() {
        Ok(cache) => cache,
        Err(e) => {
            warn!("Unable to lock cache {}", e);
            return None;
        }
    };
    if let Some(env) = cache.get(sha) {
        info!("Cache Hit {}", sha);
        if let Some(custom_metrics) = &state.dd_metrics {
            custom_metrics.incr(CustomMetrics::CacheHit.metrics_name(), request_url);
        }
        Some(env.clone())
    } else {
        info!("Cache Miss {}", sha);
        if let Some(custom_metrics) = &state.dd_metrics {
            custom_metrics.incr(CustomMetrics::CacheMiss.metrics_name(), request_url);
        }
        match state.config_dir.lock() {
            Ok(repo) => {
                if let Some(sha) = repo.refresh(remote, Some(sha)) {
                    let envs = format_envs(&repo.find(state.environments_regex.clone()));
                    if !envs.is_empty() {
                        info!("Loading envs for {}", sha);
                        cache.insert(sha, Arc::new(envs));
                    } else {
                        info!("No envs found for {}", sha);
                        return None;
                    }
                };
            }
            Err(e) => {
                warn!("Unable to lock repository {}", e);
                return None;
            }
        };
        if let Some(envs) = cache.get(sha) {
            Some(envs.clone())
        } else {
            info!("Unable to find the configuration sha {}", sha);
            None
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
