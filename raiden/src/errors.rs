use std::{error, fmt};

#[derive(Debug, Clone)]
pub struct RaidenError {
	pub msg: String,
}

impl fmt::Display for RaidenError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.msg)
	}
}

impl error::Error for RaidenError {
	fn source(&self) -> Option<&(dyn error::Error + 'static)> {
		// Generic error, underlying cause isn't tracked.
		None
	}
}

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

#[derive(Debug, Clone)]
pub struct ChannelError {
	pub msg: String,
}

impl fmt::Display for ChannelError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.msg)
	}
}

impl error::Error for ChannelError {
	fn source(&self) -> Option<&(dyn error::Error + 'static)> {
		// Generic error, underlying cause isn't tracked.
		None
	}
}

#[derive(Debug, Clone)]
pub struct TypeError {
	pub msg: String,
}

impl fmt::Display for TypeError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.msg)
	}
}

impl error::Error for TypeError {
	fn source(&self) -> Option<&(dyn error::Error + 'static)> {
		// Generic error, underlying cause isn't tracked.
		None
	}
}
