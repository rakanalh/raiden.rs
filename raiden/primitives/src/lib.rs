#![warn(clippy::missing_docs_in_private_items)]

//! Primitives crate defines various primitive Raiden data types and utils commonly used.

/// Base constants.
pub mod constants;
/// Base deserializers.
pub mod deserializers;
/// Base hashing functions.
pub mod hashing;
/// Base trait implementations.
pub mod impls;
/// Base packing functions.
pub mod packing;
/// Payment status collection.
pub mod payments;
/// Base serializers.
pub mod serializers;
/// Private key and signing utils.
pub mod signing;
#[cfg(test)]
mod tests;
/// Base traits.
pub mod traits;
/// Base types some of which are aliases from rust-web3.
pub mod types;
