use failure::Error;
use find_file_paths;
use git2::Repository;
use json_patch::merge;
use regex::Regex;
use serde_json::{self, Value};
use std::fs::File;
use std::path::{Path, PathBuf};
use tempfile::{self, TempDir};
use url::{ParseError, Url};
use walkdir::WalkDir;
use git;

pub enum ConfigDir {
    File {
        directory: PathBuf,
    },
    Git {
        git_repo: Repository,
        temp_dir: TempDir,
        directory: PathBuf,
    },
}

impl ConfigDir {
    pub fn new(src: String, ssh_key_path: &Path) -> Result<ConfigDir, Error> {
        let config_dir = if src.contains(".git") {
            let git_url = git::GitUrl::new(&src);
            let temp_dir = tempfile::tempdir()?;

            let git_repo = git_url.clone(temp_dir.path(), Some(ssh_key_path))?;

            let directory = match git_repo.workdir() {
                Some(workdir) => workdir.join(git_url.internal_path),
                None => bail!("No working directory found for git repository"),
            };

            Ok(ConfigDir::Git {
                git_repo,
                temp_dir,
                directory,
            })
        } else {
            match Url::parse(&src) {
                Ok(url) => match url.scheme() {
                    "file" => ConfigDir::new(src.replacen("file://", "", 1), ssh_key_path),
                    scheme => bail!("URL scheme {} not yet supported", scheme),
                },
                Err(ParseError::RelativeUrlWithoutBase) => Ok(ConfigDir::File {
                    directory: PathBuf::from(src),
                }),
                Err(e) => Err(e.into()),
            }
        };

        if let &Ok(ref config_dir) = &config_dir {
            if !config_dir.directory().is_dir() {
                bail!(
                    "{:?} either does not exist or is not a directory. It needs to be both",
                    config_dir.directory()
                )
            }
        }

        config_dir
    }

    pub fn directory(&self) -> &Path {
        match *self {
            ConfigDir::File { ref directory, .. } => directory,
            ConfigDir::Git { ref directory, .. } => directory,
        }
    }

    // TODO: Implement being able to re-checkout a git repo
    pub fn refresh(&self) -> &Self {
        match *self {
            ConfigDir::File { .. } => {}
            ConfigDir::Git { .. } => {}
        }

        self
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
        let config_dir = ConfigDir::new(
            String::from("file://./tests/fixtures/configs"),
            Path::new(""),
        ).unwrap();
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
        let config_dir = ConfigDir::new(
            String::from("file://./tests/fixtures/configs"),
            Path::new(""),
        ).unwrap();
        let environments = config_dir.find(
            RegexBuilder::new(r#"config\.test\d?\.json"#)
                .case_insensitive(true)
                .build()
                .unwrap(),
        );
        assert_eq!(environments.len(), 2)
    }
}
