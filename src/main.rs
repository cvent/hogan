#![warn(unused)]

#[macro_use]
extern crate failure;
extern crate hogan;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quicli;
extern crate regex;
#[macro_use]
extern crate rouille;
extern crate shellexpand;
#[macro_use]
extern crate structopt;

extern crate url;

use failure::Error;
use hogan::config::{self, ConfigDir};
use hogan::template::{self, Template};
use regex::{Regex, RegexBuilder};
use rouille::Response;
use rouille::input::plain_text_body;
use std::fs::File;
use std::io::{Cursor, Write};
use std::mem::replace;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use structopt::StructOpt;
use url::Url;

/// Transform templates with handlebars
#[derive(StructOpt, Debug)]
#[structopt(raw(setting = "structopt::clap::AppSettings::InferSubcommands"))]
struct App {
    /// Sets the level of verbosity
    #[structopt(short = "v", long = "verbose", parse(from_occurrences), raw(global = "true"))]
    verbosity: u64,

    /// URL to find configs
    #[structopt(short = "c", long = "configUrl", raw(global = "true"))]
    config_url: Url,

    /// SSH key to use in case of config_url requiring authentication
    #[structopt(short = "s", long = "sshKey", parse(from_str = "App::parse_path_buf"),
                default_value = "~/.ssh/id_rsa", raw(global = "true"))]
    ssh_key: PathBuf,

    #[structopt(subcommand)]
    cmd: AppCommand,
}

#[derive(StructOpt, Debug)]
enum AppCommand {
    /// Transform handlebars template files against config files
    #[structopt(name = "transform")]
    Transform {
        /// The relative path to where the templates are located (recursive)
        #[structopt(short = "r", long = "rootDir", parse(from_os_str), default_value = ".")]
        root_dir: PathBuf,

        /// The environment(s) to update configs for (regex accepted)
        #[structopt(short = "e", long = "environment", default_value = ".+",
                    parse(try_from_str = "App::parse_regex"))]
        environment: Regex,

        /// The templates to use (regex accepted)
        #[structopt(short = "t", long = "templateRegex",
                    default_value = "template([-.].+)?\\..+",
                    parse(try_from_str = "App::parse_regex"))]
        template_regex: Regex,
    },
    /// Respond to HTTP requests to transform a template
    #[structopt(name = "server")]
    Server {
        /// The port to serve requests on
        #[structopt(short = "p", long = "port")]
        port: u16,
    },
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

main!(|args: App, log_level: verbosity| {
    println!("{:?}", &args);
    match args.cmd {
        AppCommand::Transform {
            root_dir,
            environment,
            template_regex,
        } => {
            let mut handlebars = hogan::transform::handlebars();

            let config_dir = ConfigDir::try_from_url(args.config_url, &args.ssh_key)?;

            let environments = config_dir.find(App::config_regex(&environment)?);
            println!("Loaded {} config file(s)", environments.len());

            let templates = template::find(&root_dir, template_regex);
            println!("Loaded {} template file(s)", templates.len());

            for template in &templates {
                handlebars
                    .register_template_file(&template.path.to_string_lossy(), &template.path)?;
            }

            for environment in environments {
                println!("Updating templates for {}", environment.environment);

                for template in &templates {
                    let template_path = template.path.to_string_lossy();
                    let path = template_path.replace("template", &environment.environment);
                    let mut file = File::create(&path)?;

                    debug!("Transforming {}", path);
                    if let Err(e) = handlebars.render_to_write(
                        &template_path,
                        &environment.config_data,
                        &mut file,
                    ) {
                        bail!("Error transforming {} due to {}", &path, e);
                    }
                }
            }
        }
        AppCommand::Server { port } => {
            let mut handlebars = hogan::transform::handlebars();

            let config_dir = ConfigDir::try_from_url(args.config_url, &args.ssh_key)?;

            let mut environments = Mutex::new(config_dir.find(Regex::new(".+")?));
            let mut config_dir = Mutex::new(config_dir);

            rouille::start_server(("0.0.0.0", port), move |request| {
                router!(request,
                    (POST) (/refresh) => {
                        match environments.lock() {
                            Ok(mut environments) => match config_dir.lock() {
                                Ok(config_dir) => {
                                    config_dir.refresh();
                                    replace(&mut environments, config_dir.find(Regex::new(".+").unwrap()));
                                    Response::empty_204()
                                }
                                Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                            },
                            Err(e) => Response::text(format!("{}", e)).with_status_code(500)
                        }
                    },
                    // Transform against all configs
                    (POST) (/transform) => {
                        match environments.lock() {
                            Ok(environments) => {
                                match request.data() {
                                    Some(data) => match request.get_param("filename") {
                                        Some(filename) => {
                                            let mut template = Template {
                                                path: PathBuf::from(filename),
                                                read: data,
                                            };

                                            match template.render_to_zip(&handlebars, &environments) {
                                                Ok(zip) => Response::from_data("application/octet-stream", zip),
                                                Err(e) => Response::text(format!("{}", e)).with_status_code(500)
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
                        match environments.lock() {
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
                        match environments.lock() {
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
});
