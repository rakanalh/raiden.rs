use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractDefError {
	#[error("Contract JSON invalid: `{0}`")]
	InvalidJson(serde_json::Error),
	#[error("ABI parsing error: `{0}`")]
	ABI(ethabi::Error),
	#[error("Contract with identifier not found")]
	SpecNotFound,
	#[error("`{0}`")]
	Other(&'static str),
}

impl From<serde_json::Error> for ContractDefError {
	fn from(e: serde_json::Error) -> Self {
		Self::InvalidJson(e)
	}
}

impl From<ethabi::Error> for ContractDefError {
	fn from(e: ethabi::Error) -> Self {
		Self::ABI(e)
	}
}
