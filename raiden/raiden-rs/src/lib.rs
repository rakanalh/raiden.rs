//! # Raiden-rs
//!
//! The Raiden Network is an off-chain scaling solution, enabling near-instant, low-fee and scalable
//! payments. It's complementary to the Ethereum Blockchain and works with any ERC20 compatible
//! token. The Raiden project is work in progress. Its goal is to research state channel technology,
//! define protocols and develop reference implementations.
//!
//! ## Quickstart: `prelude`
//!
//! A prelude is provided which imports all the important data types and traits for you. Use this
//! when you want to quickly bootstrap a new project.
//!
//! ```rust
//! use raiden-rs::prelude::*;
//! ```
//!
//! ## Modules
//!
//! The following paragraphs are a quick explanation of each module in ascending order of
//! abstraction.
//!
//! ### `raiden_api`
//!
//! A high level API crate which lets you interact with the components of Raiden to trigger various
//! Raiden specific functionality such as opening / closing channels, deposit & withdraw as well as
//! initiating payments .. etc.
//!
//! ### `raiden_blockchain`
//!
//! Implements various ethereum specific functionality such as interacting with the contracts
//! on-chain, signing & recovery and decoding Ethereum events into state changes.
//!
//! ### `raiden_client`
//!
//! Uses all above crates to create a fully functional Raiden client.
//!
//! ### `raiden_macros`
//!
//! Provides simple macros for type conversions.
//!
//! ### `raiden_network_messages`, `raiden_network_transport`
//!
//! Implements Raiden protocol messages and matrix network integration to exchange messages between
//! nodes over the wire.
//!
//! ### `raiden_pathfinding`
//!
//! Implements ways to interact with the pathfinding service to retrieve routes for payments.
//!
//! ### `raiden_primitives`
//!
//! Defines various primitive data types and utils.
//!
//! ### `raiden_state_machine`
//!
//! This is the most vital crate which handles a complete chain state and it's transitions using
//! state changes.
//!
//! ### `raiden_transition`
//!
//! Plays a middleman role by handling all incoming messages and dispatching those as state changes
//! into the state machine, while also handling resulting events from the state machine to be sent
//! over the networking layer.

#![warn(missing_debug_implementations, missing_docs, rust_2018_idioms, unreachable_pub)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms), allow(dead_code, unused_variables))))]

#[doc(inline)]
pub use raiden_api;
#[doc(inline)]
pub use raiden_blockchain;
#[doc(inline)]
pub use raiden_client;
#[doc(inline)]
pub use raiden_macros;
#[doc(inline)]
pub use raiden_network_messages;
#[doc(inline)]
pub use raiden_network_transport;
#[doc(inline)]
pub use raiden_pathfinding;
#[doc(inline)]
pub use raiden_primitives;
#[doc(inline)]
pub use raiden_state_machine;
#[doc(inline)]
pub use raiden_transition;

/// Easy imports of frequently used type definitions and traits.
#[doc(hidden)]
#[allow(unknown_lints, ambiguous_glob_reexports)]
pub mod prelude {
	pub use raiden_macros::*;
	pub use raiden_primitives::*;
}

// For macro expansions only, not public API.
#[doc(hidden)]
#[allow(unused_extern_crates)]
extern crate self as raiden;
