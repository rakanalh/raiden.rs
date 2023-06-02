#![warn(clippy::missing_docs_in_private_items)]

use std::{
	error,
	fmt,
};

/// The state transition error type.
#[derive(Debug, Clone)]
pub struct StateTransitionError {
	pub msg: String,
}

impl fmt::Display for StateTransitionError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.msg)
	}
}

impl error::Error for StateTransitionError {
	fn source(&self) -> Option<&(dyn error::Error + 'static)> {
		// Generic error, underlying cause isn't tracked.
		None
	}
}

impl Into<StateTransitionError> for String {
	fn into(self) -> StateTransitionError {
		StateTransitionError { msg: self }
	}
}
