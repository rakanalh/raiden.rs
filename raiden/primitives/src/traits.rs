#![warn(clippy::missing_docs_in_private_items)]

/// Convert type for bytes
pub trait ToBytes {
	fn to_bytes(&self) -> Vec<u8>;
}

/// Convert type to string
pub trait Stringify {
	fn as_string(&self) -> String;
}

/// Checksum an address
pub trait Checksum {
	fn checksum(&self) -> String;
}

/// Return the pex format of an address
pub trait ToPexAddress {
	fn pex(&self) -> String;
}
