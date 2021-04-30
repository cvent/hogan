use crate::error::HoganError;
use crate::find_file_paths;
use crate::git;
use anyhow::{Context, Result};
use json_patch::merge;
use regex::Regex;
use regex::RegexBuilder;
use serde_json::{self, Value};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::{self, FromStr};
use tempfile::{self, TempDir};
use url::{ParseError, Url};
use walkdir::WalkDir;

#[derive(Debug, PartialEq, Clone)]
pub enum ConfigUrl {
    File {
        path: PathBuf,
    },
    Git {
        url: Url,
        branch: Option<String>,
        internal_path: PathBuf,
    },
}

impl FromStr for ConfigUrl {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match Url::parse(&s) {
            Ok(url) => {
                if url.scheme() == "file" {
                    Ok(ConfigUrl::File {
                        path: PathBuf::from(s.trim_start_matches("file://")),
                    })
                } else {
                    let path_segments = url
                        .path_segments()
                        .ok_or_else(|| HoganError::InvalidConfiguration {
                            param: "url".to_string(),
                            msg: "Url cannot be a base".to_string(),
                        })?
                        .map(|segment| segment.to_owned())
                        .collect::<Vec<String>>();

                    match path_segments
                        .iter()
                        .position(|s| s.ends_with(".git"))
                        .map(|index| index + 1)
                    {
                        Some(git_index) => {
                            let mut git_url = url.clone();
                            git_url.set_fragment(None);

                            let internal_path = if git_index > path_segments.len() {
                                PathBuf::new()
                            } else {
                                let (base_segments, rest) = path_segments.split_at(git_index);

                                git_url
                                    .path_segments_mut()
                                    .map_err(|_| HoganError::InvalidConfiguration {
                                        param: "url".to_string(),
                                        msg: "Url cannot be a base".to_string(),
                                    })?
                                    .clear()
                                    .extend(base_segments);

                                rest.iter().collect()
                            };

                            Ok(ConfigUrl::Git {
                                url: git_url,
                                branch: url.fragment().map(|f| f.to_owned()),
                                internal_path,
                            })
                        }
                        None => Err(HoganError::InvalidConfiguration {
                            msg: "Config Url not a file path, and not a .git URL".to_string(),
                            param: "url".to_string(),
                        }
                        .into()),
                    }
                }
            }
            Err(ParseError::RelativeUrlWithoutBase) => if s.contains(".git") {
                format!("ssh://{}", str::replace(s, ":", "/"))
            } else {
                format!("file://{}", s)
            }
            .parse(),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug)]
pub enum ConfigDir {
    File {
        directory: PathBuf,
    },
    Git {
        url: Url,
        head_sha: String,
        ssh_key_path: PathBuf,
        temp_dir: TempDir,
        directory: PathBuf,
        native_git: bool,
    },
}

impl ConfigDir {
    pub fn new(url: ConfigUrl, ssh_key_path: &Path, native_git: bool) -> Result<ConfigDir> {
        let config_dir = match url {
            ConfigUrl::Git {
                url,
                internal_path,
                branch,
            } => {
                let temp_dir = tempfile::tempdir().map_err(|e| HoganError::GitError {
                    msg: format!("Unable to create temp directory {:?}", e),
                })?;

                let git_repo = if native_git {
                    git::ext_clone(&url, temp_dir.path())?;
                    git::build_repo(temp_dir.path().to_str().unwrap())?
                } else {
                    git::clone(
                        &url,
                        branch.as_deref(),
                        temp_dir.path(),
                        Some(&ssh_key_path),
                    )?
                };

                let head_sha = git::get_head_sha(&git_repo)?;

                let directory = match git_repo.workdir() {
                    Some(workdir) => workdir.join(internal_path),
                    None => {
                        return Err(HoganError::GitError {
                            msg: "No working directory found for git repository".to_string(),
                        }
                        .into())
                    }
                };
                let ssh_key_path = ssh_key_path.to_owned();

                Ok(ConfigDir::Git {
                    url,
                    head_sha,
                    ssh_key_path,
                    temp_dir,
                    directory,
                    native_git,
                })
            }
            ConfigUrl::File { path } => Ok(ConfigDir::File { directory: path }),
        };

        if let Ok(ref config_dir) = config_dir {
            if !config_dir.directory().is_dir() {
                return Err(HoganError::UnknownError {
                    msg: format!(
                        "{:?} either does not exist or is not a directory. It needs to be both",
                        config_dir.directory()
                    ),
                }
                .into());
            }
        }

        config_dir
    }

    pub fn extend(&self, branch: &str) -> Result<ConfigDir> {
        match self {
            ConfigDir::Git {
                url,
                ssh_key_path,
                native_git,
                ..
            } => ConfigDir::new(
                ConfigUrl::Git {
                    url: url.clone(),
                    branch: Some(branch.to_owned()),
                    internal_path: PathBuf::new(),
                },
                ssh_key_path,
                *native_git,
            ),
            ConfigDir::File { .. } => Err(HoganError::GitError {
                msg: "Can not extend file config".to_string(),
            }
            .into()),
        }
    }

    pub fn directory(&self) -> &Path {
        match *self {
            ConfigDir::File { ref directory, .. } => directory,
            ConfigDir::Git { ref directory, .. } => directory,
        }
    }

    pub fn refresh(
        &self,
        remote: Option<&str>,
        target: Option<&str>,
        allow_fetch: bool,
    ) -> Result<String> {
        match self {
            ConfigDir::File { .. } => Err(HoganError::GitError {
                msg: "Cannot refresh a file config".to_string(),
            }
            .into()),
            ConfigDir::Git {
                directory,
                url,
                ssh_key_path,
                ..
            } => {
                let git_repo = git::build_repo(directory.to_str().unwrap())
                    .with_context(|| "Attempting to refresh git repo -- Building Repo")?;

                git::reset(
                    &git_repo,
                    remote.unwrap_or("origin"),
                    Some(ssh_key_path),
                    Some(url),
                    target,
                    false,
                    allow_fetch,
                )
                .with_context(|| format!("Error refreshing to {:?}", target))
            }
        }
    }

    pub fn find(&self, filter: Regex) -> Vec<Environment> {
        fn find_env_type_data<'a>(types: &'a [EnvironmentType], name: &str) -> &'a Value {
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

    fn find_environments(&self, filter: Regex) -> Box<dyn Iterator<Item = Environment>> {
        Box::new(
            find_file_paths(self.directory(), filter)
                .filter_map(|p| File::open(p).ok())
                .filter_map(|f| serde_json::from_reader(f).ok())
                .filter_map(|c: Config| c.into_environment()),
        )
    }

    fn find_environment_types(&self) -> Box<dyn Iterator<Item = EnvironmentType>> {
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
                        .and_then(|c: Config| c.into_environment_type())
                        .map(|mut e| {
                            e.environment_type = env_type;
                            e
                        })
                }),
        )
    }

    pub fn find_branch_head(
        &self,
        remote_name: &str,
        branch_name: &str,
        refresh: bool,
    ) -> Result<String> {
        match self {
            ConfigDir::File { .. } => Err(HoganError::GitError {
                msg: "Unable to perform git actions on a file".to_string(),
            })
            .context("Finding branch head"),
            ConfigDir::Git {
                directory,
                ssh_key_path,
                url,
                ..
            } => {
                let git_repo = git::build_repo(directory.to_str().unwrap())
                    .with_context(|| "Finding branch head")?;

                if refresh {
                    git::fetch(&git_repo, remote_name, Some(ssh_key_path), Some(url))
                        .with_context(|| "Finding branch head, updating repo")?;
                }

                git::find_branch_head(&git_repo, &format!("{}/{}", remote_name, branch_name))
                    .with_context(|| "Finding branch head, querying for head")
            }
        }
    }

    pub fn perform_maintenance(&self) -> Result<()> {
        match self {
            ConfigDir::File { .. } => Err(HoganError::GitError {
                msg: "Unable to perform git actions on a file".to_string(),
            })
            .context("Performing repo maintenance"),
            ConfigDir::Git {
                directory,
                native_git,
                ..
            } => {
                if *native_git {
                    git::ext_maintenance(&directory.as_path())
                        .with_context(|| "Performing Maintenance")?;
                }
                Ok(())
            }
        }
    }

    pub fn fetch_only(&self, remote_name: &str) -> Result<()> {
        match self {
            ConfigDir::File { .. } => Err(HoganError::GitError {
                msg: "Unable to perform git actions on a file".to_string(),
            })
            .context("Fetching git repo"),
            ConfigDir::Git {
                directory,
                ssh_key_path,
                url,
                native_git,
                ..
            } => {
                if *native_git {
                    git::ext_fetch(&directory.as_path(), remote_name)
                        .with_context(|| "Fetching git repo")?;
                    Ok(())
                } else {
                    let git_repo = git::build_repo(directory.to_str().unwrap())
                        .with_context(|| "Fetching git repo. Building repo")?;
                    git::fetch(&git_repo, remote_name, Some(ssh_key_path), Some(url))
                        .with_context(|| "Fetching Repo")?;
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Config {
    Environment(Environment),
    EnvironmentType(EnvironmentType),
}

impl Config {
    fn into_environment(self) -> Option<Environment> {
        match self {
            Config::Environment(e) => Some(e),
            _ => None,
        }
    }

    fn into_environment_type(self) -> Option<EnvironmentType> {
        match self {
            Config::EnvironmentType(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Environment {
    pub environment: String,
    pub environment_type: Option<String>,
    pub config_data: Value,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
struct EnvironmentType {
    environment_type: String,
    config_data: Value,
}

pub fn build_regex(pattern: &str) -> Result<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| {
            HoganError::InvalidConfiguration {
                param: "regex".to_string(),
                msg: format!("Regex Error: {:?}", e),
            }
            .into()
        })
}

pub fn build_env_regex(env: &str, base_pattern: Option<&str>) -> Result<Regex> {
    let pattern = match base_pattern {
        Some(base) => {
            let raw = String::from(base);
            raw.replace("{}", env)
        }
        //Format only supports string literals
        None => format!(r"^config\.{}\.json$", env),
    };
    debug!("Searching for environment with filename: {}", pattern);
    build_regex(&pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_github_url() {
        assert_eq!(
            "git@github.com:foo/bar.git".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::Git {
                url: Url::parse("ssh://git@github.com/foo/bar.git").unwrap(),
                branch: None,
                internal_path: PathBuf::from(""),
            }
        );

        assert_eq!(
            "git@github.com:foo/bar.git/internal/path#branch"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("ssh://git@github.com/foo/bar.git").unwrap(),
                branch: Some(String::from("branch")),
                internal_path: PathBuf::from("internal/path"),
            }
        );

        assert_eq!(
            "https://github.com/foo/bar.git"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("https://github.com/foo/bar.git").unwrap(),
                branch: None,
                internal_path: PathBuf::from(""),
            }
        );

        assert_eq!(
            "https://github.com/foo/bar.git/internal/path#branch"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("https://github.com/foo/bar.git").unwrap(),
                branch: Some(String::from("branch")),
                internal_path: PathBuf::from("internal/path"),
            }
        );
    }

    #[test]
    fn test_bitbucket_git_url() {
        assert_eq!(
            "ssh://git@bitbucket.org/foo/bar.git"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("ssh://git@bitbucket.org/foo/bar.git").unwrap(),
                branch: None,
                internal_path: PathBuf::from(""),
            }
        );

        assert_eq!(
            "ssh://git@bitbucket.org/foo/bar.git/internal/path#branch"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("ssh://git@bitbucket.org/foo/bar.git").unwrap(),
                branch: Some(String::from("branch")),
                internal_path: PathBuf::from("internal/path"),
            }
        );

        assert_eq!(
            "https://username@bitbucket.org/scm/foo/bar.git"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("https://username@bitbucket.org/scm/foo/bar.git").unwrap(),
                branch: None,
                internal_path: PathBuf::from(""),
            }
        );

        assert_eq!(
            "https://username@bitbucket.org/scm/foo/bar.git/internal/path#branch"
                .parse::<ConfigUrl>()
                .unwrap(),
            ConfigUrl::Git {
                url: Url::parse("https://username@bitbucket.org/scm/foo/bar.git").unwrap(),
                branch: Some(String::from("branch")),
                internal_path: PathBuf::from("internal/path"),
            }
        );
    }

    #[test]
    fn test_local_path() {
        assert_eq!(
            "foo/bar/baz".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::File {
                path: PathBuf::from("foo/bar/baz"),
            }
        );

        assert_eq!(
            "/foo/bar/baz".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::File {
                path: PathBuf::from("/foo/bar/baz"),
            }
        );

        assert_eq!(
            "foo/bar.git".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::Git {
                url: Url::parse("ssh://foo/bar.git").unwrap(),
                branch: None,
                internal_path: PathBuf::from("")
            }
        );

        assert_eq!(
            "foo/bar.git/baz".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::Git {
                url: Url::parse("ssh://foo/bar.git").unwrap(),
                branch: None,
                internal_path: PathBuf::from("baz")
            }
        );

        assert_eq!(
            "file://foo/bar/baz".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::File {
                path: PathBuf::from("foo/bar/baz"),
            }
        );

        assert_eq!(
            "file://foo/bar.git/baz".parse::<ConfigUrl>().unwrap(),
            ConfigUrl::File {
                path: PathBuf::from("foo/bar.git/baz"),
            }
        );
    }

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
            "file://./tests/fixtures/configs".parse().unwrap(),
            Path::new(""),
            true,
        )
        .unwrap();
        let environments = config_dir.find(build_regex("config\\..+\\.json$").unwrap());
        assert_eq!(environments.len(), 4)
    }

    #[test]
    fn test_find_subset_configs() {
        let config_dir = ConfigDir::new(
            "file://./tests/fixtures/configs".parse().unwrap(),
            Path::new(""),
            true,
        )
        .unwrap();
        let environments = config_dir.find(build_regex(r#"config\.test\d?\.json"#).unwrap());
        assert_eq!(environments.len(), 2)
    }
}
