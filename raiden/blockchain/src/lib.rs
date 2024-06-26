//! Implements various ethereum specific functionality such as interacting with the contracts
//! on-chain, signing & recovery and decoding Ethereum events into state changes.
/// Constants module.
pub mod constants;
/// Contracts module.
pub mod contracts;
/// Blockchain events decode module.
pub mod decode;
/// Errors module
pub mod errors;
/// Events module.
pub mod events;
/// Filters module.
pub mod filters;
/// Keys module.
pub mod keys;
/// Proxies module.
pub mod proxies;
/// Secret module.
pub mod secret;
/// Transactions module.
pub mod transactions;
