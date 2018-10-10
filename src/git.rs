use failure::Error;
use git2::build::RepoBuilder;
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use std::path::Path;
use url::Url;

pub fn clone(
    url: &Url,
    branch: Option<&str>,
    path: &Path,
    ssh_key_path: Option<&Path>,
) -> Result<Repository, Error> {
    let mut callbacks = RemoteCallbacks::new();

    if let Some(password) = url.password() {
        debug!("Using password auth");
        callbacks.credentials(move |_url, username_from_url, _allowed_types| {
            Cred::userpass_plaintext(username_from_url.unwrap(), password)
        });
    } else if let Some(ssh_key_path) = ssh_key_path {
        debug!("Using SSH auth");
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
    repo_builder.clone(url.as_str(), path).map_err(|e| e.into())
}
