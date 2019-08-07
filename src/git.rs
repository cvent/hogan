use failure::Error;
use git2::build::RepoBuilder;
use git2::{AutotagOption, Cred, FetchOptions, RemoteCallbacks, Repository};
use std::path::Path;
use std::str;
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

    callbacks.transfer_progress(|stats| {
        if stats.received_objects() == stats.total_objects() {
            let step = stats.total_objects() / 10;
            if step == 0
                || stats.indexed_objects() % step == 0
                || stats.total_objects() == stats.indexed_objects()
            {
                info!(
                    "Resolving deltas {}/{}",
                    stats.indexed_deltas(),
                    stats.total_deltas()
                );
            }
        } else if stats.total_objects() > 0 {
            let step = stats.total_objects() / 10;
            if step == 0
                || stats.received_objects() % step == 0
                || stats.total_objects() == stats.received_objects()
            {
                info!(
                    "Received {}/{} objects ({}) in {} bytes",
                    stats.received_objects(),
                    stats.total_objects(),
                    stats.indexed_objects(),
                    stats.received_bytes()
                );
            }
        }
        true
    });

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

fn make_ssh_auth(ssh_key_path: &Path) -> RemoteCallbacks {
    let mut callback = RemoteCallbacks::new();
    callback.credentials(move |_url, username_from_url, _allowed_types| {
        Cred::ssh_key(username_from_url.unwrap(), None, ssh_key_path, None)
    });

    callback
}

fn make_password_auth(url: &Url) -> RemoteCallbacks {
    if let Some(password) = url.password() {
        let mut callback = RemoteCallbacks::new();
        callback.credentials(move |_url, username_from_url, _allowed_type| {
            Cred::userpass_plaintext(username_from_url.unwrap(), password)
        });
        callback
    } else {
        RemoteCallbacks::new()
    }
}

fn detach_head(repo: &Repository, sha: &str) -> Result<(), Error> {
    let sha_oid = match repo.revparse_single(sha) {
        Ok(revspec) => {
            info!("Found revision {}", sha);
            revspec.id()
        }
        Err(e) => {
            warn!("Unable to resolve reference {}", sha);
            return Err(e.into());
        }
    };
    info!("Switching repo head to {}", sha);
    repo.set_head_detached(sha_oid).map_err(|e| e.into())
}

fn fetch(
    repo: &Repository,
    remote: &str,
    ssh_key_path: Option<&Path>,
    url: Option<&Url>,
) -> Result<(), Error> {
    let mut cb = if let Some(s) = ssh_key_path {
        make_ssh_auth(s)
    } else if let Some(u) = url {
        make_password_auth(u)
    } else {
        RemoteCallbacks::default()
    };
    let mut remote = repo
        .find_remote(remote)
        .or_else(|_| repo.remote_anonymous(remote))?;
    cb.sideband_progress(|data| {
        info!("remote: {}", str::from_utf8(data).unwrap());
        true
    });

    cb.transfer_progress(|stats| {
        if stats.received_objects() == stats.total_objects() {
            let step = stats.total_objects() / 10;
            if step == 0
                || stats.indexed_objects() % step == 0
                || stats.indexed_objects() == stats.total_objects()
            {
                info!(
                    "Resolving deltas {}/{}",
                    stats.indexed_deltas(),
                    stats.total_deltas()
                );
            }
        } else if stats.total_objects() > 0 {
            let step = stats.total_objects() / 10;
            if step == 0
                || stats.received_objects() % step == 0
                || stats.received_objects() == stats.total_objects()
            {
                info!(
                    "Received {}/{} objects ({}) in {} bytes",
                    stats.received_objects(),
                    stats.total_objects(),
                    stats.indexed_objects(),
                    stats.received_bytes()
                );
            }
        }
        true
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);
    remote.download(&[], Some(&mut fo))?;

    remote.disconnect();

    remote.update_tips(None, true, AutotagOption::Unspecified, None)?;

    Ok(())
}

pub fn reset(
    repo: &Repository,
    remote: &str,
    ssh_key_path: Option<&Path>,
    url: Option<&Url>,
    sha: Option<&str>,
    force_refresh: bool,
) -> Result<String, Error> {
    if force_refresh {
        fetch(repo, remote, ssh_key_path, url)?;
    };

    if let Some(sha) = sha {
        match detach_head(repo, sha) {
            Ok(_) => {}
            Err(_) => {
                info!("Couldn't find {}. Trying to refreshing repo", sha);
                fetch(repo, remote, ssh_key_path, url)?;
                match detach_head(repo, sha) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("Unable to find ref {}: {:?}", sha, e);
                        return Err(e);
                    }
                }
            }
        }
    }

    get_head_sha(repo)
}

pub fn build_repo(path: &str) -> Result<Repository, Error> {
    Repository::open(path).map_err(|e| e.into())
}

pub fn get_head_sha(repo: &Repository) -> Result<String, Error> {
    let head = repo.head()?;
    if let Some(target) = head.target() {
        let sha = target.to_string();
        Ok(sha[..7].to_string())
    } else {
        Err(format_err!("Unable to find HEAD of repo"))
    }
}
