#![warn(unused)]

#[macro_use]
extern crate failure;
extern crate hogan;
#[macro_use]
extern crate log;
extern crate loggerv;
extern crate regex;
#[macro_use]
extern crate structopt;

use failure::Error;
use hogan::config::environments;
use hogan::template::templates;
use loggerv::Logger;
use regex::{Regex, RegexBuilder};
use std::fs::File;
use std::path::PathBuf;
use std::process;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct App {
    /// The relative path to where the configuration files are located
    #[structopt(long = "configDir", parse(from_os_str))]
    pub config_dir: PathBuf,

    /// The regex to use when looking for config files
    #[structopt(long = "configRegex")]
    pub config_regex: Option<Regex>,

    /// The relative path to where the templates are located (recursive)
    #[structopt(long = "rootDir", parse(from_os_str), default_value = ".")]
    pub root_dir: PathBuf,

    /// The region to update configs for (regex is accepted)
    #[structopt(long = "region", default_value = ".+", parse(try_from_str = "App::parse_regex"))]
    pub region: Regex,

    /// The regex to use when looking for template files
    #[structopt(long = "templateRegex", default_value = "(.*\\.)?template(\\.Release|\\-liquibase|\\-quartz)?\\.([Cc]onfig|yaml|properties)$", parse(try_from_str = "App::parse_regex"))]
    pub template_regex: Regex,

    /// Sets the level of verbosity
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbosity: u64,
}

impl App {
    fn new() -> App {
        let mut opts = App::from_args();

        if opts.config_regex.is_none() {
            opts.config_regex = Regex::new(&format!("config\\.{}\\.json$", opts.region)).ok();
        }

        opts
    }

    fn parse_regex(src: &str) -> Result<Regex, Error> {
        RegexBuilder::new(src)
            .case_insensitive(true)
            .build()
            .map_err(|e| e.into())
    }
}

fn main() {
    fn run() -> Result<(), Error> {
        let opts = App::new();

        Logger::new()
            .verbosity(opts.verbosity + 1)
            .level(false)
            .module_path(true)
            .init()?;

        let mut handlebars = hogan::transform::handlebars();

        let environments = environments(&opts.config_dir, opts.config_regex.unwrap());
        info!("Loaded {} config file(s)", environments.len());

        let template_paths = templates(&opts.root_dir, opts.template_regex);
        info!("Loaded {} template file(s)", template_paths.len());

        for template_path in &template_paths {
            handlebars.register_template_file(&template_path.to_string_lossy(), template_path)?;
        }

        for environment in environments {
            info!("Updating templates for {}", environment.environment);

            for template_path in &template_paths {
                let template_path = template_path.to_string_lossy();
                let path = template_path.replace("template", &environment.environment);
                let mut file = File::create(&path)?;

                debug!("Transforming {}", path);
                if let Err(e) =
                    handlebars.render_to_write(&template_path, &environment.config_data, &mut file)
                {
                    bail!("Error transforming {} due to {}", &path, e);
                }
            }
        }

        Ok(())
    }

    if let Err(e) = run() {
        error!("{:?}", e);
        process::exit(1);
    }
}
