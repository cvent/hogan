#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use crate::app::cli;
use crate::app::config::{App, AppCommand};
use crate::app::server;
use anyhow::{Context, Result};

use structopt::StructOpt;

mod app;

fn main() -> Result<()> {
    let opt = App::from_args();

    stderrlog::new()
        .module(module_path!())
        .verbosity(opt.verbosity + 2)
        .timestamp(stderrlog::Timestamp::Millisecond)
        .init()
        .with_context(|| "Error initializing logging")?;

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
            fetch_poller,
            allow_fetch,
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
                fetch_poller,
                allow_fetch,
            )?;
        }
    }

    Ok(())
}
