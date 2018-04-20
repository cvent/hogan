#![warn(unused)]
#![feature(proc_macro, wasm_import_module, wasm_custom_section)]

#[macro_use]
extern crate failure;
extern crate hogan;
#[macro_use]
extern crate log;
extern crate loggerv;
extern crate regex;
extern crate structopt;
extern crate wasm_bindgen;

use failure::Error;
use hogan::config::environments;
use hogan::generate_configs;
use hogan::template::templates;
use loggerv::Logger;
use regex::{Regex, RegexBuilder};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process;
use structopt::StructOpt;
use wasm_bindgen::prelude::*;

#[derive(StructOpt, Debug)]
pub struct Args {
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
    #[structopt(long = "region", default_value = ".+", parse(try_from_str = "Args::parse_regex"))]
    pub region: Regex,

    /// The regex to use when looking for template files
    #[structopt(long = "templateRegex", default_value = "(.*\\.)?template(\\.Release|\\-liquibase|\\-quartz)?\\.([Cc]onfig|yaml|properties)$", parse(try_from_str = "Args::parse_regex"))]
    pub template_regex: Regex,

    /// Sets the level of verbosity
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbosity: u64,
}

impl Args {
    fn from_args() -> Args {
        <Args as StructOpt>::from_args().set_dynamic_defaults()
    }

    pub fn from_vec(vec: Vec<OsString>) -> Args {
        Args::from_iter(vec.into_iter()).set_dynamic_defaults()
    }

    fn set_dynamic_defaults(mut self) -> Self {
        if self.config_regex.is_none() {
            self.config_regex = Regex::new(&format!("config\\.{}\\.json$", self.region)).ok();
        }

        self
    }

    fn parse_regex(src: &str) -> Result<Regex, Error> {
        RegexBuilder::new(src)
            .case_insensitive(true)
            .build()
            .map_err(|e| e.into())
    }
}

fn main() {
    if let Err(e) = run(Args::from_args()) {
        error!("{:?}", e);
        process::exit(1);
    }
}

#[wasm_bindgen]
pub fn wasm_main(args: String) {
    if let Err(e) = run(Args::from_iter(args.split(" "))) {
        error!("{:?}", e);
        process::exit(1);
    }
}

pub fn run(args: Args) -> Result<(), Error> {
    Logger::new()
        .verbosity(args.verbosity + 1)
        .level(false)
        .module_path(true)
        .init()?;

    let mut handlebars = hogan::transform::handlebars();

    let environments = environments(&args.config_dir, args.config_regex.unwrap());
    info!("Loaded {} config file(s)", environments.len());

    let template_paths = templates(&args.root_dir, args.template_regex);
    info!("Loaded {} template file(s)", template_paths.len());

    generate_configs(&mut handlebars, environments, template_paths)
}
