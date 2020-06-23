#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;

use crate::app::cli;
use crate::app::config::{App, AppCommand};
use crate::app::server;
use failure::Error;
use structopt;
use structopt::StructOpt;

mod app;

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
            cli::cli(
                templates_path,
                environments_regex,
                templates_regex,
                common,
                ignore_existing,
            )?;
        }
        AppCommand::Server {
            common,
            port,
            address,
            cache_size,
            environments_regex,
            datadog,
            environment_pattern,
            db_path,
        } => {
            server::start_up_server(
                common,
                port,
                address,
                cache_size,
                environments_regex,
                datadog,
                environment_pattern,
                db_path,
            )?;
        }
    }

    Ok(())
}
