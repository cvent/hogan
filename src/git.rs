use failure::Error;
use git2::build::RepoBuilder;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use std::path::Path;

pub fn clone(
    url: &str,
    branch: Option<&str>,
    path: &Path,
    ssh_key_path: Option<&Path>,
) -> Result<Repository, Error> {
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

    if let Some(branch) = branch {
        debug!("Setting branch to {}", branch);
        repo_builder.branch(branch);
    }

    info!("Cloning to {:?}", path);
    repo_builder.clone(&url, path).map_err(|e| e.into())
}
