use failure::Error;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use git2::build::RepoBuilder;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub struct GitUrl {
    // Cannot use real URLs here because of https://github.com/servo/rust-url/issues/220
    pub url: String,
    pub branch: Option<String>,
    pub internal_path: PathBuf,
}

impl GitUrl {
    pub fn new(url: &str) -> GitUrl {
        let (base_url, rest) = {
            let split_by_git = url.splitn(2, ".git").collect::<Vec<&str>>();
            if split_by_git.len() == 2 {
                (format!("{}.git", split_by_git[0]), split_by_git[1])
            } else {
                (String::from(url), "")
            }
        };

        let (mut path, branch) = {
            let split_by_hash = rest.splitn(2, "#").collect::<Vec<&str>>();
            if split_by_hash.len() == 2 {
                (split_by_hash[0], Some(String::from(split_by_hash[1])))
            } else {
                (rest, None)
            }
        };

        if path.starts_with("/") {
            path = &path[1..];
        }

        GitUrl {
            url: base_url,
            branch,
            internal_path: PathBuf::from(path),
        }
    }

    pub fn clone(&self, path: &Path, ssh_key_path: Option<&Path>) -> Result<Repository, Error> {
        let mut callbacks = RemoteCallbacks::new();
        if let Some(ssh_key_path) = ssh_key_path {
            callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                Cred::ssh_key(username_from_url.unwrap(), None, ssh_key_path, None)
            });
        }

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        let mut repo_builder = RepoBuilder::new();
        repo_builder.fetch_options(fetch_options);

        if let Some(ref branch) = self.branch {
            println!("Setting branch to {}", branch);
            repo_builder.branch(branch);
        }

        info!("Cloning to {:?}", path);
        repo_builder.clone(&self.url, path).map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_github_url() {
        assert_eq!(
            GitUrl::new("git@github.com:cvent/hogan.git"),
            GitUrl {
                url: String::from("git@github.com:cvent/hogan.git"),
                branch: None,
                internal_path: PathBuf::from(""),
            }
        );
    }

    #[test]
    fn test_ssh_github_url_with_extras() {
        assert_eq!(
            GitUrl::new("git@github.com:cvent/hogan.git/internal/path#branch"),
            GitUrl {
                url: String::from("git@github.com:cvent/hogan.git"),
                branch: Some(String::from("branch")),
                internal_path: PathBuf::from("internal/path"),
            }
        );
    }

    #[test]
    fn test_https_github_url() {
        assert_eq!(
            GitUrl::new("https://github.com/cvent/hogan.git"),
            GitUrl {
                url: String::from("https://github.com/cvent/hogan.git"),
                branch: None,
                internal_path: PathBuf::from(""),
            }
        );
    }

    #[test]
    fn test_https_github_url_with_extras() {
        assert_eq!(
            GitUrl::new("https://github.com/cvent/hogan.git/internal/path#branch"),
            GitUrl {
                url: String::from("https://github.com/cvent/hogan.git"),
                branch: Some(String::from("branch")),
                internal_path: PathBuf::from("internal/path"),
            }
        );
    }
}
