#![warn(clippy::missing_docs_in_private_items)]

pub trait ToBytes {
	fn to_bytes(&self) -> Vec<u8>;
}

pub trait Stringify {
	fn as_string(&self) -> String;
}

pub trait ToChecksummed {
	fn to_checksummed(&self) -> String;
}

pub trait ToPexAddress {
	fn pex(&self) -> String;
}
