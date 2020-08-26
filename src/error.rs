use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum HoganError {
    #[error("There was an error with the underlying git repository. {msg}")]
    GitError { msg: String },
    #[error("The requested SHA {sha} was not found in the git repo")]
    UnknownSHA { sha: String },
    #[error("The requested branch {branch} was not found in the git repo")]
    UnknownBranch { branch: String },
    #[error("The requested environment {env} was not found in {sha}")]
    UnknownEnvironment { sha: String, env: String },
    #[error("There was a problem with the provided template")]
    InvalidTemplate { msg: String },
    #[error("The request was malformed")]
    BadRequest,
    #[error("Request timed out due to internal congestion")]
    InternalTimeout,
    #[error("An error occurred parsing configuration {param}: {msg}")]
    InvalidConfiguration { param: String, msg: String },
    #[error("An unknown error occurred. {msg}")]
    UnknownError { msg: String },
}

impl From<git2::Error> for HoganError {
    fn from(e: git2::Error) -> Self {
        HoganError::GitError {
            msg: e.message().to_owned(),
        }
    }
}

impl From<anyhow::Error> for HoganError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast() {
            Ok(e) => e,
            Err(e) => {
                warn!("Bad cast to a HoganError {:?}", e);
                HoganError::UnknownError {
                    msg: format!("Error {:?}", e),
                }
            }
        }
    }
}
