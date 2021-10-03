use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid URL `{0}`")]
    Uri(&'static str),
    #[error("`{0}`")]
    Http(hyper::Error),
    #[error("`{0}`")]
    Serialization(serde_json::Error),
    #[error("Error: `{0}`")]
    Other(String),
}
