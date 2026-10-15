use crate::http::HttpError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("crawl file: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Update(#[from] spiderlens_engine::crawl::UpdateError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("credential store: {0}")]
    Credentials(String),
    #[error("sign-in: {0}")]
    Auth(String),
    #[error("no connection for {0}; connect it in Settings › Integrations first")]
    NoConnection(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
