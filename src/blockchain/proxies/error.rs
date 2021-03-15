use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Broken precondition: `{0}`")]
    BrokenPrecondition(String),
    #[error("Blockchain error: `{0}`")]
    Web3(web3::Error),
    #[error("Blockchain error: `{0}`")]
    ChainError(web3::contract::Error),
    #[error("Unrecoverable error: `{0}`")]
    Unrecoverable(String),
}

impl From<web3::Error> for ProxyError {
    fn from(e: web3::Error) -> Self {
        Self::Web3(e)
    }
}

impl From<web3::contract::Error> for ProxyError {
    fn from(e: web3::contract::Error) -> Self {
        Self::ChainError(e)
    }
}
