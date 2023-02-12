use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
	#[error("Insufficient balance: `{0}`")]
	InsufficientEth(String),
	#[error("Broken precondition: `{0}`")]
	BrokenPrecondition(String),
	#[error(transparent)]
	Web3(#[from] web3::Error),
	#[error(transparent)]
	ChainError(#[from] web3::contract::Error),
	#[error("Recoverable error: `{0}`")]
	Recoverable(String),
	#[error("Unrecoverable error: `{0}`")]
	Unrecoverable(String),
}
