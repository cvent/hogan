#![warn(unused)]

#[macro_use]
extern crate failure;
extern crate hogan;
#[macro_use]
extern crate log;
extern crate regex;
#[macro_use]
extern crate rouille;
extern crate shellexpand;
extern crate stderrlog;
#[macro_use]
extern crate structopt;

use failure::Error;
use hogan::config::ConfigDir;
use hogan::template::{Template, TemplateDir};
use regex::{Regex, RegexBuilder};
use rouille::input::plain_text_body;
use rouille::Response;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::mem::replace;
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
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
            default_value = "template([-.].+)?\\.(config|ya?ml|properties)",
            value_name = "REGEX"
        )]
        templates_regex: Regex,
    },
    /// Respond to HTTP requests to transform a template
    #[structopt(name = "server")]
    Server {
        #[structopt(flatten)]
        common: AppCommon,

        /// Port to serve requests on
        #[structopt(
            short = "p",
            long = "port",
            default_value = "80",
            value_name = "PORT"
        )]
        port: u16,
    },
}

#[derive(StructOpt, Debug)]
struct AppCommon {
    /// Config source. Accepts file and git URLs. Paths within a git repository may be appended
    /// to a git URL, and branches may be specified as a URL fragment (recursive if applicable)
    #[structopt(short = "c", long = "configs", value_name = "URL")]
    configs_url: String,

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
        } => {
            let mut handlebars = hogan::transform::handlebars(common.strict);

            let template_dir = TemplateDir::new(templates_path)?;
            let mut templates = template_dir.find(templates_regex);
            println!("Loaded {} template file(s)", templates.len());

            let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;
            let environments = config_dir.find(App::config_regex(&environments_regex)?);
            println!("Loaded {} config file(s)", environments.len());

            for environment in environments {
                println!("Updating templates for {}", environment.environment);

                for mut template in &mut templates {
                    debug!("Transforming {:?}", template.path);

                    let rendered = template.render(&handlebars, &environment)?;
                    trace!("Rendered: {:?}", rendered.contents);
                    if let Err(e) = File::create(&rendered.path)?.write_all(&rendered.contents) {
                        bail!("Error transforming {:?} due to {}", rendered.path, e);
                    }
                }
            }
        }
        AppCommand::Server { common, port } => {
            let mut handlebars = hogan::transform::handlebars(common.strict);

            let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;

            let mut environments = RwLock::new(config_dir.find(Regex::new(".+")?));
            let mut config_dir = Mutex::new(config_dir);

            rouille::start_server(("0.0.0.0", port), move |request| {
                router!(request,
                    (POST) (/refresh) => {
                        match environments.write() {
                            Ok(mut environments) => match config_dir.lock() {
                                Ok(config_dir) => {
                                    replace(environments.deref_mut(), config_dir.refresh().find(Regex::new(".+").unwrap()));
                                    Response::empty_204()
                                }
                                Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                            },
                            Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                        }
                    },
                    // Transform against all configs
                    (POST) (/transform) => {
                        match environments.read() {
                            Ok(environments) => {
                                match request.data() {
                                    Some(mut data) => match request.get_param("filename") {
                                        Some(filename) => {
                                            let mut buffer = String::new();

                                            match data.read_to_string(&mut buffer) {
                                                Ok(_) => {
                                                    let template = Template {
                                                        path: PathBuf::from(filename),
                                                        contents: buffer,
                                                    };

                                                    match template.render_to_zip(&handlebars, &environments) {
                                                        Ok(zip) => Response::from_data("application/octet-stream", zip),
                                                        Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                                                    }
                                                },
                                                Err(_) => Response::text("Cannot read body").with_status_code(400)
                                            }
                                        },
                                        None => Response::text("Query parameter 'filename' required").with_status_code(400)
                                    },
                                    None => Response::text("POST body required").with_status_code(400)
                                }
                            },
                            Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                        }
                    },
                    // Transform against a single config
                    (POST) (/transform/{env: String}) => {
                        match environments.read() {
                            Ok(environments) => match environments.iter().find(|e| e.environment == env) {
                                Some(env) => {
                                    let body = try_or_400!(plain_text_body(request));
                                    println!("Transforming {}", body);
                                    match handlebars.render_template(&body, &env.config_data) {
                                        Ok(rendered) => Response::text(rendered),
                                        Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                                    }
                                },
                                None => Response::empty_404()
                            },
                            Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                        }
                    },
                    // Return a single config
                    (GET) (/config/{env: String}) => {
                        match environments.read() {
                            Ok(environments) => match environments.iter().find(|e| e.environment == env) {
                                Some(env) => Response::json(env),
                                None => Response::empty_404()
                            },
                            Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                        }
                    },
                    // default route
                    _ => Response::empty_404()
                )
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate assert_cli;
    extern crate dir_diff;
    extern crate fs_extra;
    extern crate tempfile;

    use self::fs_extra::dir;

    use std::path::Path;

    #[cfg(not(all(target_env = "msvc", target_arch = "x86_64")))]
    #[test]
    fn test_transform() {
        let temp_dir = tempfile::tempdir().unwrap();

        fs_extra::copy_items(
            &vec!["tests/fixtures/projects/templates"],
            temp_dir.path(),
            &dir::CopyOptions::new(),
        ).unwrap();

        let templates_path = temp_dir.path().join("templates");

        assert_cli::Assert::main_binary()
            .with_args(&[
                "transform",
                "--configs",
                "tests/fixtures/configs",
                "--templates",
                templates_path.to_str().unwrap(),
            ])
            .stdout()
            .contains(format!(r#"Finding Files: {:?}"#, templates_path))
            .stdout()
            .contains(r"regex: /template([-.].+)?\.(config|ya?ml|properties)/")
            .stdout()
            .contains("Loaded 3 template file(s)")
            .stdout()
            .contains(r#"Finding Files: "tests/fixtures/configs""#)
            .stdout()
            .contains(r#"regex: /config\..+\.json$/"#)
            .stdout()
            .contains("Loaded 4 config file(s)")
            .stdout()
            .contains("Updating templates for EMPTY")
            .stdout()
            .contains("Updating templates for ENVTYPE")
            .stdout()
            .contains("Updating templates for TEST")
            .stdout()
            .contains("Updating templates for TEST2")
            .unwrap();

        assert!(
            !dir_diff::is_different(
                &templates_path.join("project-1"),
                &Path::new("tests/fixtures/projects/rendered/project-1")
            ).unwrap()
        );

        assert!(
            !dir_diff::is_different(
                &templates_path.join("project-2"),
                &Path::new("tests/fixtures/projects/rendered/project-2")
            ).unwrap()
        );
    }
}
