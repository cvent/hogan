use anyhow::Result;
use hogan::config::ConfigUrl;
use regex::{Regex, RegexBuilder};
use std::path::PathBuf;
use structopt::clap::AppSettings;
use structopt::StructOpt;

/// Transform templates with handlebars
#[derive(StructOpt, Debug)]
#[structopt(setting = AppSettings::InferSubcommands)]
pub struct App {
    /// Sets the level of verbosity
    #[structopt(short = "v", long = "verbose", parse(from_occurrences), global = true)]
    pub verbosity: usize,

    #[structopt(subcommand)]
    pub cmd: AppCommand,
}

#[derive(StructOpt, Debug)]
pub enum AppCommand {
    /// Transform handlebars template files against config files
    #[structopt(name = "transform")]
    Transform {
        #[structopt(flatten)]
        common: AppCommon,

        /// Filter environments to render templates for
        #[structopt(
            short = "e",
            long = "environments-filter",
            parse(try_from_str = App::parse_regex),
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
            parse(try_from_str = App::parse_regex),
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

        /// Set the oldest age of items to keep in the caches (in days)
        #[structopt(
            long = "db-age",
            default_value = "120",
            value_name = "DB_MAX_AGE_IN_DAYS"
        )]
        db_max_age: usize,

        /// Set the size of the in memory cache
        #[structopt(long = "cache-size", default_value = "64", value_name = "CACHE_SIZE")]
        cache_size: usize,

        /// Filter environments to render templates for
        #[structopt(
            short = "e",
            long = "environments-filter",
            parse(try_from_str = App::parse_regex),
            default_value = ".+",
            value_name = "REGEX"
        )]
        environments_regex: Regex,

        /// Activate datadog metrics
        #[structopt(short = "d", long = "datadog")]
        datadog: bool,

        /// Pattern used when matching a singular environment. Must contain a {} which represents where the env name will be inserted
        #[structopt(
            long = "env-pattern",
            default_value = r"^config\.{}\.json$",
            value_name = "PATTERN"
        )]
        environment_pattern: String,

        ///Filepath to the embedded db for storing environments. Will be created if it doesn't exist. If not provided a
        /// random temp directory will be created
        #[structopt(long = "db", value_name = "PATH", default_value = "hogan.db")]
        db_path: String,

        ///The delay between background fetches against the git repo
        #[structopt(
            long = "fetch-poller",
            default_value = "10000",
            value_name = "MILLISECONDS"
        )]
        fetch_poller: u64,

        ///If enabled, will allow requests to trigger fetches against the git repo. Otherwise a background poller will perform the fetches
        ///on a set interval
        #[structopt(short, long)]
        allow_fetch: bool,
    },
}

#[derive(StructOpt, Debug, Clone)]
pub struct AppCommon {
    /// Config source. Accepts file and git URLs. Paths within a git repository may be appended
    /// to a git URL, and branches may be specified as a URL fragment (recursive if applicable)
    #[structopt(short = "c", long = "configs", value_name = "URL")]
    pub configs_url: ConfigUrl,

    /// SSH key to use if configs URL requires authentication
    #[structopt(
        short = "k",
        long = "ssh-key",
        parse(from_str = App::parse_path_buf),
        default_value = "~/.ssh/id_rsa",
        value_name = "FILE"
    )]
    pub ssh_key: PathBuf,

    /// Throw errors if values do not exist in configs
    #[structopt(short = "s", long = "strict")]
    pub strict: bool,

    /// When enabled the application will assume an external git executable is installed and
    /// available in the execution environment. This will enable maintenance on the underlying git repo
    /// This flag must be set to utilize the other native git operations (clone and fetch)
    #[structopt(short = "g", long = "git")]
    pub native_git: bool,

    /// When enabled the application will use the external git executable for performing fetches
    #[structopt(long = "git-fetch")]
    pub native_fetch: bool,

    /// When enabled the application will use the external git executable for performing clones
    #[structopt(long = "git-clone")]
    pub native_clone: bool,
}

impl App {
    pub fn config_regex(environment: &Regex) -> Result<Regex> {
        App::parse_regex(&format!("config\\.{}\\.json$", environment))
    }

    pub fn parse_regex(src: &str) -> Result<Regex> {
        RegexBuilder::new(src)
            .case_insensitive(true)
            .build()
            .map_err(|e| e.into())
    }

    pub fn parse_path_buf(src: &str) -> PathBuf {
        PathBuf::from(shellexpand::tilde(src).into_owned())
    }
}
