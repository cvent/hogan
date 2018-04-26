use failure::Error;
use find_file_paths;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use git2::build::RepoBuilder;
use json_patch::merge;
use regex::Regex;
use serde_json::{self, Value};
use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};
use tempfile::{self, TempDir};
use url::Url;
use walkdir::WalkDir;

pub enum ConfigDir {
    File {
        url: Url,
        directory: PathBuf,
    },
    Git {
        url: Url,
        git_repo: Repository,
        temp_dir: TempDir,
        directory: PathBuf,
    },
    Http {
        url: Url,
        temp_dir: TempDir,
        directory: PathBuf,
    },
}

impl ConfigDir {
    pub fn try_from_url(url: Url, ssh_key_path: &Path) -> Result<ConfigDir, Error> {
        if url.scheme() == "file" {
            let directory = match url.to_file_path() {
                Ok(path) => path,
                Err(_) => {
                    let cwd = env::current_dir()?;
                    println!("CWD: {:?}", cwd);

                    let path = url.path();
                    println!("Path: {}", path);
                    cwd.join(&path[1..])
                }
            };

            Ok(ConfigDir::File { url, directory })
        } else if url.path().contains(".git") {
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|url, username_from_url, allowed_types| {
                Cred::ssh_key(username_from_url.unwrap(), None, ssh_key_path, None)
            });

            let mut fetch_options = FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);

            let temp_dir = tempfile::tempdir()?;
            println!("Cloning to {:?}", temp_dir);
            let git_repo = RepoBuilder::new()
                .fetch_options(fetch_options)
                .clone(url.as_str(), temp_dir.path())?;

            let directory = git_repo.workdir().unwrap().to_path_buf();

            Ok(ConfigDir::Git {
                url,
                git_repo,
                temp_dir,
                directory,
            })
        } else {
            bail!("Cannot get directory from {}", url);
        }
    }

    pub fn directory(&self) -> &Path {
        match *self {
            ConfigDir::File { ref directory, .. } => directory,
            ConfigDir::Git { ref directory, .. } => directory,
            ConfigDir::Http { ref directory, .. } => directory,
        }
    }

    pub fn refresh(&self) {
        match *self {
            ConfigDir::File { .. } => {}
            ConfigDir::Git { ref git_repo, .. } => {}
            ConfigDir::Http { ref url, .. } => {}
        }
    }

    pub fn find(&self, filter: Regex) -> Vec<Environment> {
        fn find_env_type_data<'a>(types: &'a Vec<EnvironmentType>, name: &str) -> &'a Value {
            types
                .iter()
                .find(|e| e.environment_type == name)
                .map(|env| &env.config_data)
                .unwrap_or(&Value::Null)
        }

        let environment_types =
            ConfigDir::find_environment_types(self).collect::<Vec<EnvironmentType>>();
        let global = find_env_type_data(&environment_types, "global");

        ConfigDir::find_environments(self, filter)
            .map(|mut environment| {
                let parent = if let Some(ref env_type_name) = environment.environment_type {
                    find_env_type_data(&environment_types, env_type_name)
                } else {
                    &Value::Null
                };

                let mut config_data = global.clone(); // Start with global
                merge(&mut config_data, &parent); // Merge in an env type
                merge(&mut config_data, &environment.config_data); // Merge with the actual config

                environment.config_data = config_data;
                environment
            })
            .collect()
    }

    fn find_environments(&self, filter: Regex) -> Box<Iterator<Item = Environment>> {
        Box::new(
            find_file_paths(self.directory(), filter)
                .filter_map(|p| File::open(p).ok())
                .filter_map(|f| serde_json::from_reader(f).ok())
                .filter_map(|c: Config| c.as_environment()),
        )
    }

    fn find_environment_types(&self) -> Box<Iterator<Item = EnvironmentType>> {
        Box::new(
            WalkDir::new(self.directory())
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter_map(|e| {
                    let path = e.path();
                    let env_type = path.file_stem().unwrap().to_string_lossy().into_owned();
                    File::open(&path)
                        .ok()
                        .and_then(|f| serde_json::from_reader(f).ok())
                        .and_then(|c: Config| c.as_environment_type())
                        .and_then(|mut e| {
                            e.environment_type = env_type;
                            Some(e)
                        })
                }),
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Config {
    Environment(Environment),
    EnvironmentType(EnvironmentType),
}

impl Config {
    fn as_environment(self) -> Option<Environment> {
        match self {
            Config::Environment(e) => Some(e),
            _ => None,
        }
    }

    fn as_environment_type(self) -> Option<EnvironmentType> {
        match self {
            Config::EnvironmentType(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Environment {
    pub environment: String,
    pub environment_type: Option<String>,
    pub config_data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EnvironmentType {
    environment_type: String,
    config_data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::RegexBuilder;

    #[test]
    fn test_basic_triple_merge() {
        let global: Value = serde_json::from_str(r#"{"a": null}"#).unwrap();
        let parent = serde_json::from_str(r#"{"a": 1}"#).unwrap();
        let doc = serde_json::from_str(r#"{"a": 2}"#).unwrap();

        let mut merged = global.clone();

        merge(&mut merged, &parent);
        merge(&mut merged, &doc);

        let expected_json: Value = serde_json::from_str(r#"{"a": 2}"#).unwrap();

        assert_eq!(merged, expected_json)
    }

    #[test]
    fn test_complex_merge() {
        let global: Value = Value::Null;
        let parent = serde_json::from_str(r#"{"a": 1, "b": null, "c": 3, "d": 4}"#).unwrap();
        let doc = serde_json::from_str(r#"{"a": null, "b": 2, "c": 4, "e": 5}"#).unwrap();

        let mut merged = global.clone();

        merge(&mut merged, &parent);
        merge(&mut merged, &doc);

        let expected_json: Value =
            serde_json::from_str(r#"{"b": 2, "c": 4, "d": 4, "e": 5}"#).unwrap();

        assert_eq!(merged, expected_json)
    }

    #[test]
    fn test_find_all_configs() {
        let config_url = Url::parse("file://./tests/fixtures/configs").unwrap();
        let config_dir = ConfigDir::try_from_url(config_url, Path::new("")).unwrap();
        let environments = config_dir.find(
            RegexBuilder::new("config\\..+\\.json$")
                .case_insensitive(true)
                .build()
                .unwrap(),
        );
        assert_eq!(environments.len(), 4)
    }

    #[test]
    fn test_find_subset_configs() {
        let config_url = Url::parse("file://./tests/fixtures/configs").unwrap();
        let config_dir = ConfigDir::try_from_url(config_url, Path::new("")).unwrap();
        let environments = config_dir.find(
            RegexBuilder::new(r#"config\.test\d?\.json"#)
                .case_insensitive(true)
                .build()
                .unwrap(),
        );
        assert_eq!(environments.len(), 2)
    }
}
