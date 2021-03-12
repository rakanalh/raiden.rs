use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("Contract JSON invalid: `{0}`")]
    InvalidJson(serde_json::Error),
    #[error("ABI parsing error: `{0}`")]
    ABI(ethabi::Error),
    #[error("Contract with identifier not found")]
    SpecNotFound,
}

impl From<serde_json::Error> for ContractError {
    fn from(e: serde_json::Error) -> Self {
        Self::InvalidJson(e)
    }
}

impl From<ethabi::Error> for ContractError {
    fn from(e: ethabi::Error) -> Self {
        Self::ABI(e)
    }
}
