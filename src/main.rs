#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use failure::Error;
use hogan;
use hogan::config::ConfigDir;
use hogan::config::ConfigUrl;
use hogan::template::{Template, TemplateDir};
use lru_time_cache::LruCache;
use regex::{Regex, RegexBuilder};
use rocket::config::Config;
use rocket::http::Status;
use rocket::{Data, State};
use rocket_contrib::json::{Json, JsonValue};
use rocket_lamb::RocketExt;
use serde::Serialize;
use shellexpand;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::ErrorKind::AlreadyExists;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use stderrlog;
use structopt;
use structopt::StructOpt;

/// Transform templates with handlebars
#[derive(StructOpt, Debug)]
#[structopt(raw(setting = "structopt::clap::AppSettings::InferSubcommands"))]
struct App {
    /// Sets the level of verbosity
    #[structopt(
        short = "v",
        long = "verbose",
        parse(from_occurrences),
        raw(global = "true")
    )]
    verbosity: usize,

    #[structopt(subcommand)]
    cmd: AppCommand,
}

#[derive(StructOpt, Debug)]
enum AppCommand {
    /// Transform handlebars template files against config files
    #[structopt(name = "transform")]
    Transform {
        #[structopt(flatten)]
        common: AppCommon,

        /// Filter environments to render templates for
        #[structopt(
            short = "e",
            long = "environments-filter",
            parse(try_from_str = "App::parse_regex"),
            default_value = ".+",
            value_name = "REGEX"
        )]
        environments_regex: Regex,

        /// Template source (recursive)
        #[structopt(
            short = "t",
            long = "templates",
            parse(from_os_str),
            default_value = ".",
            value_name = "DIR"
        )]
        templates_path: PathBuf,

        /// Filter templates to transform
        #[structopt(
            short = "f",
            long = "templates-filter",
            parse(try_from_str = "App::parse_regex"),
            default_value = "^[^.]*(\\w+\\.)*template([-.].+)?\\.(config|ya?ml|properties)",
            value_name = "REGEX"
        )]
        templates_regex: Regex,

        /// Ignore existing config files intead of overwriting
        #[structopt(short = "i", long = "ignore-existing")]
        ignore_existing: bool,
    },
    /// Respond to HTTP requests to transform a template
    #[structopt(name = "server")]
    Server {
        #[structopt(flatten)]
        common: AppCommon,

        /// Port to serve requests on
        #[structopt(short = "p", long = "port", default_value = "80", value_name = "PORT")]
        port: u16,

        /// The address for the server to bind on
        #[structopt(
            short = "b",
            long = "address",
            default_value = "0.0.0.0",
            value_name = "ADDRESS"
        )]
        address: String,

        /// If enabled, configures the server to handle requests as a lambda behind an API Gateway Proxy
        /// See: https://github.com/GREsau/rocket-lamb
        #[structopt(long = "lambda")]
        lambda: bool,

        /// Set the size of the SHA LRU cache
        #[structopt(long = "cache", default_value = "100", value_name = "CACHE_SIZE")]
        cache_size: usize,

        /// Filter environments to render templates for
        #[structopt(
            short = "e",
            long = "environments-filter",
            parse(try_from_str = "App::parse_regex"),
            default_value = ".+",
            value_name = "REGEX"
        )]
        environments_regex: Regex,
    },
}

#[derive(StructOpt, Debug)]
struct AppCommon {
    /// Config source. Accepts file and git URLs. Paths within a git repository may be appended
    /// to a git URL, and branches may be specified as a URL fragment (recursive if applicable)
    #[structopt(short = "c", long = "configs", value_name = "URL")]
    configs_url: ConfigUrl,

    /// SSH key to use if configs URL requires authentication
    #[structopt(
        short = "k",
        long = "ssh-key",
        parse(from_str = "App::parse_path_buf"),
        default_value = "~/.ssh/id_rsa",
        value_name = "FILE"
    )]
    ssh_key: PathBuf,

    /// Throw errors if values do not exist in configs
    #[structopt(short = "s", long = "strict")]
    strict: bool,
}

impl App {
    fn config_regex(environment: &Regex) -> Result<Regex, Error> {
        App::parse_regex(&format!("config\\.{}\\.json$", environment))
    }

    fn parse_regex(src: &str) -> Result<Regex, Error> {
        RegexBuilder::new(src)
            .case_insensitive(true)
            .build()
            .map_err(|e| e.into())
    }

    fn parse_path_buf(src: &str) -> PathBuf {
        PathBuf::from(shellexpand::tilde(src).into_owned())
    }
}

fn main() -> Result<(), Error> {
    let opt = App::from_args();

    stderrlog::new()
        .module(module_path!())
        .verbosity(opt.verbosity + 2)
        .init()?;

    match opt.cmd {
        AppCommand::Transform {
            templates_path,
            environments_regex,
            templates_regex,
            common,
            ignore_existing,
        } => {
            let handlebars = hogan::transform::handlebars(common.strict);

            let template_dir = TemplateDir::new(templates_path)?;
            let mut templates = template_dir.find(templates_regex);
            println!("Loaded {} template file(s)", templates.len());

            let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;
            let environments = config_dir.find(App::config_regex(&environments_regex)?);
            println!("Loaded {} config file(s)", environments.len());

            for environment in environments {
                println!("Updating templates for {}", environment.environment);

                for template in &mut templates {
                    debug!("Transforming {:?}", template.path);

                    let rendered = template.render(&handlebars, &environment)?;
                    trace!("Rendered: {:?}", rendered.contents);

                    if ignore_existing {
                        if let Err(e) = match OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&rendered.path)
                        {
                            Ok(ref mut f) => f.write_all(&rendered.contents),
                            Err(ref e) if e.kind() == AlreadyExists => {
                                println!("Skipping {:?} - config already exists.", rendered.path);
                                trace!("Skipping {:?} - config already exists.", rendered.path);
                                Ok(())
                            }
                            Err(e) => Err(e),
                        } {
                            bail!("Error transforming {:?} due to {}", rendered.path, e)
                        }
                    } else if let Err(e) =
                        File::create(&rendered.path)?.write_all(&rendered.contents)
                    {
                        bail!("Error transforming {:?} due to {}", rendered.path, e)
                    }
                }
            }
        }
        AppCommand::Server {
            common,
            port,
            address,
            cache_size,
            lambda,
            environments_regex,
        } => {
            let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;

            let environments = Mutex::new(
                LruCache::<String, Vec<hogan::config::Environment>>::with_capacity(cache_size),
            );

            init_cache(&environments, &environments_regex, &config_dir)?;
            let config_dir = Mutex::new(config_dir);

            info!("Starting server on {}:{}", address, port);
            let state = ServerState {
                environments,
                config_dir,
                environments_regex,
                strict: common.strict,
            };
            start_server(address, port, lambda, state)?;
        }
    }

    Ok(())
}

struct ServerState {
    environments: Mutex<LruCache<String, Vec<hogan::config::Environment>>>,
    config_dir: Mutex<hogan::config::ConfigDir>,
    environments_regex: Regex,
    strict: bool,
}

fn start_server(address: String, port: u16, lambda: bool, state: ServerState) -> Result<(), Error> {
    let mut config = Config::development();
    config.set_port(port);
    config.set_address(address)?;
    let server = rocket::custom(config)
        .mount(
            "/",
            routes![
                health_check,
                get_envs,
                get_config_by_env,
                transform_env,
                transform_all_envs,
                get_branch_sha,
            ],
        )
        .manage(state);
    if lambda {
        server.lambda().launch();
    } else {
        server.launch();
    }
    Ok(())
}

#[get("/ok")]
fn health_check() -> Status {
    Status::Ok
}

#[post("/transform/<sha>/<env>", data = "<body>")]
fn transform_env(
    body: Data,
    sha: String,
    env: String,
    state: State<ServerState>,
) -> Result<String, Status> {
    let sha = format_sha(&sha);
    match get_env(
        &state.environments,
        &state.config_dir,
        None,
        sha,
        &state.environments_regex,
    ) {
        Some(environments) => match environments.iter().find(|e| e.environment == env) {
            Some(env) => {
                let handlebars = hogan::transform::handlebars(state.strict);
                let mut data = String::new();
                body.open().read_to_string(&mut data).map_err(|e| {
                    warn!("Unable to consume transform body: {:?}", e);
                    Status::InternalServerError
                })?;
                handlebars
                    .render_template(&data, &env.config_data)
                    .map_err(|_| Status::BadRequest)
            }
            None => Err(Status::NotFound),
        },
        None => Err(Status::NotFound),
    }
}

#[post("/transform/<sha>?<filename>", data = "<body>")]
fn transform_all_envs(
    sha: String,
    filename: String,
    body: Data,
    state: State<ServerState>,
) -> Result<Vec<u8>, Status> {
    let sha = format_sha(&sha);
    match get_env(
        &state.environments,
        &state.config_dir,
        None,
        &sha,
        &state.environments_regex,
    ) {
        Some(environments) => {
            let handlebars = hogan::transform::handlebars(state.strict);
            let mut data = String::new();
            body.open()
                .read_to_string(&mut data)
                .map_err(|e| {
                    warn!("Unable to consume transform body: {:?}", e);
                    Status::InternalServerError
                })
                .map_err(|e| {
                    warn!("Unable to read request body {:?}", e);
                    Status::InternalServerError
                })?;
            let template = Template {
                path: PathBuf::from(filename),
                contents: data,
            };
            template
                .render_to_zip(&handlebars, &environments)
                .map_err(|e| {
                    warn!("Unable to make zip file: {:?}", e);
                    Status::InternalServerError
                })
        }
        None => Err(Status::NotFound),
    }
}

#[get("/envs/<sha>")]
fn get_envs(sha: String, state: State<ServerState>) -> Result<JsonValue, Status> {
    let mut cache = match state.environments.lock() {
        Ok(cache) => cache,
        Err(e) => {
            warn!("Unable to lock cache: {:?}", e);
            return Err(Status::NotFound);
        }
    };
    if let Some(envs) = cache.get(&sha) {
        let env_list = format_envs(envs);
        Ok(json!(env_list))
    } else {
        match state.config_dir.lock() {
            Ok(repo) => {
                if let Some(sha) = repo.refresh(None, Some(&sha)) {
                    cache.insert(sha.to_owned(), repo.find(state.environments_regex.clone()));
                };
            }
            Err(e) => {
                warn!("Unable to lock repository {:?}", e);
                return Err(Status::NotFound);
            }
        }
        if let Some(envs) = cache.get(&sha) {
            let env_list = format_envs(envs);
            Ok(json!(env_list))
        } else {
            Err(Status::NotFound)
        }
    }
}

#[get("/config/<sha>/<env>")]
fn get_config_by_env(
    sha: String,
    env: String,
    state: State<ServerState>,
) -> Result<JsonValue, Status> {
    let sha = format_sha(&sha);
    match get_env(
        &state.environments,
        &state.config_dir,
        None,
        sha,
        &state.environments_regex,
    ) {
        Some(environments) => match environments.iter().find(|e| e.environment == env) {
            Some(env) => Ok(json!(env)),
            None => Err(Status::NotFound),
        },
        None => {
            warn!("Error getting environments");
            Err(Status::InternalServerError)
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShaResponse {
    head_sha: String,
    branch_name: String,
}

#[get("/heads/<branch_name>?<remote_name>")]
fn get_branch_sha(
    remote_name: Option<String>,
    branch_name: String,
    state: State<ServerState>,
) -> Result<Json<ShaResponse>, Status> {
    if let Ok(config_dir) = state.config_dir.lock() {
        if let Some(head_sha) = config_dir.find_branch_head(
            &remote_name.unwrap_or_else(|| String::from("origin")),
            &branch_name,
        ) {
            Ok(Json(ShaResponse {
                head_sha,
                branch_name,
            }))
        } else {
            Err(Status::NotFound)
        }
    } else {
        warn!("Error locking git repo");
        Err(Status::InternalServerError)
    }
}

fn init_cache(
    cache: &Mutex<LruCache<String, Vec<hogan::config::Environment>>>,
    environments_regex: &Regex,
    repo: &hogan::config::ConfigDir,
) -> Result<(), Error> {
    match repo {
        ConfigDir::Git { head_sha, .. } => {
            let mut cache = cache.lock().unwrap();
            info!("Initializing cache to: {}", head_sha);
            cache.insert(head_sha.clone(), repo.find(environments_regex.clone()));
            Ok(())
        }
        ConfigDir::File { .. } => Err(format_err!("Cannot change file based configuration")),
    }
}

fn get_env(
    cache: &Mutex<LruCache<String, Vec<hogan::config::Environment>>>,
    repo: &Mutex<hogan::config::ConfigDir>,
    remote: Option<&str>,
    sha: &str,
    environments_regex: &Regex,
) -> Option<Vec<hogan::config::Environment>> {
    let mut cache = match cache.lock() {
        Ok(cache) => cache,
        Err(e) => {
            warn!("Unable to lock cache {}", e);
            return None;
        }
    };
    if let Some(envs) = cache.get(sha) {
        Some(envs.clone())
    } else {
        match repo.lock() {
            Ok(repo) => {
                if let Some(sha) = repo.refresh(remote, Some(sha)) {
                    cache.insert(sha.to_owned(), repo.find(environments_regex.clone()));
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

fn format_envs(envs: &[hogan::config::Environment]) -> Vec<HashMap<&str, &String>> {
    let mut env_list = Vec::new();
    for env in envs.iter() {
        let mut env_map = HashMap::new();
        env_map.insert("Name", &env.environment);
        if let Some(environment_type) = &env.environment_type {
            env_map.insert("Type", environment_type);
        }
        env_list.push(env_map);
    }

    env_list
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
    use assert_cmd;
    use dir_diff;
    use fs_extra;
    use predicates;
    use tempfile;

    use self::assert_cmd::prelude::*;
    use self::fs_extra::dir;
    use self::predicates::prelude::*;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;

    #[cfg(not(all(target_env = "msvc", target_arch = "x86_64")))]
    #[test]
    fn test_transform() {
        let temp_dir = tempfile::tempdir().unwrap();

        fs_extra::copy_items(
            &vec!["tests/fixtures/projects/templates"],
            temp_dir.path(),
            &dir::CopyOptions::new(),
        )
        .unwrap();

        let templates_path = temp_dir.path().join("templates");

        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

        let cmd = cmd.args(&[
            "transform",
            "--configs",
            "tests/fixtures/configs",
            "--templates",
            templates_path.to_str().unwrap(),
        ]);

        cmd.assert().success();

        cmd.assert().stdout(
            predicate::str::contains(format!(r#"Finding Files: {:?}"#, templates_path)).from_utf8(),
        );

        cmd.assert().stdout(
            predicate::str::contains(
                r"regex: /^[^.]*(\w+\.)*template([-.].+)?\.(config|ya?ml|properties)/",
            )
            .from_utf8(),
        );

        cmd.assert()
            .stdout(predicate::str::contains("Loaded 6 template file(s)").from_utf8());

        cmd.assert().stdout(
            predicate::str::contains(r#"Finding Files: "tests/fixtures/configs""#).from_utf8(),
        );

        cmd.assert()
            .stdout(predicate::str::contains(r#"regex: /config\..+\.json$/"#).from_utf8());

        cmd.assert()
            .stdout(predicate::str::contains("Loaded 4 config file(s)").from_utf8());

        for environment in ["EMPTY", "ENVTYPE", "TEST", "TEST2"].iter() {
            cmd.assert().stdout(
                predicate::str::contains(format!("Updating templates for {}", environment))
                    .from_utf8(),
            );
        }

        assert!(!dir_diff::is_different(
            &templates_path.join("project-1"),
            &Path::new("tests/fixtures/projects/rendered/project-1")
        )
        .unwrap());

        assert!(!dir_diff::is_different(
            &templates_path.join("project-2"),
            &Path::new("tests/fixtures/projects/rendered/project-2")
        )
        .unwrap());
    }

    #[cfg(not(all(target_env = "msvc", target_arch = "x86_64")))]
    #[test]
    fn test_ignore_existing() {
        let temp_dir = tempfile::tempdir().unwrap();

        fs_extra::copy_items(
            &vec!["tests/fixtures/projects/templates"],
            temp_dir.path(),
            &dir::CopyOptions::new(),
        )
        .unwrap();

        let templates_path = temp_dir.path().join("templates");

        let ignore_path = templates_path.join("project-1/Web.EMPTY.config");
        if let Ok(ref mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&ignore_path)
        {
            f.write_all(b"Hamburger.")
                .expect("Failed to create test file for ignore.")
        }

        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        let cmd = cmd.args(&[
            "transform",
            "--configs",
            "tests/fixtures/configs",
            "--templates",
            templates_path.to_str().unwrap(),
            "-i",
        ]);

        cmd.assert().success();

        // assert that running the command with the ignore flag
        // did not overwrite the manually created project-1/Web.EMPTY.config
        let data2 =
            std::fs::read_to_string(&ignore_path).expect("Failed to read test file for ignore.");
        assert!(data2 == "Hamburger.");

        // after running the command again without the ignore flag
        // assert that the configs now match those in the rendered directory
        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        let cmd = cmd.args(&[
            "transform",
            "--configs",
            "tests/fixtures/configs",
            "--templates",
            templates_path.to_str().unwrap(),
        ]);
        cmd.assert().success();

        assert!(!dir_diff::is_different(
            &templates_path.join("project-1"),
            &Path::new("tests/fixtures/projects/rendered/project-1")
        )
        .unwrap());

        assert!(!dir_diff::is_different(
            &templates_path.join("project-2"),
            &Path::new("tests/fixtures/projects/rendered/project-2")
        )
        .unwrap());
    }
}
