# Raiden.rs

[![CircleCI](https://dl.circleci.com/status-badge/img/gh/rakanalh/raiden.rs/tree/main.svg?style=svg)](https://dl.circleci.com/status-badge/redirect/gh/rakanalh/raiden.rs/tree/main)
[![codecov](https://codecov.io/gh/rakanalh/raiden.rs/branch/main/graph/badge.svg?token=Jyi76olOco)](https://codecov.io/gh/rakanalh/raiden.rs)
[![Crates.io Version](https://img.shields.io/crates/v/raiden-rs)](https://crates.io/crates/raiden-rs)
[![docs.rs](https://img.shields.io/docsrs/raiden-rs)](https://docs.rs/raiden-rs/)

The **UNOFFICIAL** Raiden client implementation in Rust

[![Raiden](https://raw.githubusercontent.com/rakanalh/raiden-rust/main/.github/images/raiden.png)](https://raiden.network/)

<h4 align="center">
  Fast, cheap, scalable token transfers for Ethereum
</h4>

#### Quicklinks

- [Official client](https://github.com/raiden-network/raiden)
- [Getting Started](https://github.com/raiden-network/raiden#getting-started)
- [Repositories](https://github.com/raiden-network/raiden#repositories)

The Raiden Network is an off-chain scaling solution, enabling near-instant, low-fee and scalable payments. It's complementary to the Ethereum Blockchain and works with any ERC20 compatible token. The Raiden project is work in progress. Its goal is to research state channel technology, define protocols and develop reference implementations.

## About this project

The project is aimed at implementing the Raiden protocol as a set of framework components which can be put together to write your own Raiden-compatible clients which can serve different purposes. Examples of such clients can be decentralized exchanges, bots among others.

Examples and documentation of the work you'll find in this repo should become available as soon as the implementation is completed.


## Documentation

In case you are new to Raiden, feel free to jump to the [official Raiden documentation](https://raiden-network.readthedocs.io/en/latest/) to learn what it is and how it works.

This project implements Raiden functionality using Rust in separate crates.

- [Primitives](https://github.com/rakanalh/raiden.rs/tree/main/raiden/primitives): Defines various primitive data types and utils.
- [State Machine](https://github.com/rakanalh/raiden.rs/tree/main/raiden/state-machine): This is the most vital crate which handles a complete chain state and it's transitions using state changes.
- [Blockchain](https://github.com/rakanalh/raiden.rs/tree/main/raiden/blockchain): Implements various ethereum specific functionality such as interacting with the contracts on-chain, signing & recovery and decoding Ethereum events into state changes.
- [Pathfinding](https://github.com/rakanalh/raiden.rs/tree/main/raiden/pathfinding): Implements ways to interact with the pathfinding service to retrieve routes for payments.
- [Networking](https://github.com/rakanalh/raiden.rs/tree/main/raiden/network): Implements Raiden protocol messages and matrix network integration to exchange messages between nodes over the wire.
- [Transition](https://github.com/rakanalh/raiden.rs/tree/main/raiden/transition): Plays a middleman role by handling all incoming messages and dispatching those as state changes into the state machine, while also handling resulting events from the state machine to be sent over the networking layer.
- [API](https://github.com/rakanalh/raiden.rs/tree/main/raiden/api): A high level API crate which lets you interact with the components of Raiden to trigger various Raiden specific functionality such as opening / closing channels, deposit & withdraw as well as initiating payments .. etc.

The `bin` directory provides examples on how to use the above components to link them together:
- [Raiden Client](https://github.com/rakanalh/raiden.rs/tree/main/bin/raiden): Uses all above crates to create a fully functional Raiden client.
- [State replayer](https://github.com/rakanalh/raiden.rs/tree/main/bin/state-replayer): Uses the state machine crate to replay state changes in Raiden's node storage to recreate the latest chain state. Very useful for debugging!
- [Token Ops](https://github.com/rakanalh/raiden.rs/tree/main/bin/token-ops): Uses the blockchain crate to interact with the Raiden token contracts.


### Raiden.rs vs Official Python Client.

Raiden.rs has been written with the idea of having different framework components in mind. Those components can be mixed and matched to build Raiden functionality that is specific to your use case while the Python client can only be used as a full node which provides REST APIs to interact with the node on a single Ethereum account. The provided REST APIs limit the possibilities of what can be implemented on top of Raiden's protocol.
For example, imagine you're implementing a custodial tipping bot where users send each other tips or small payments (This is a very populate fun project that is almost built for every blockchain there is). Provided your use case, that would mean that your service has to manage multiple accounts. Instead of running a single full node for each client, you can implement a single node which handles multiple accounts. The way to do that is by:
- Each account would have it's own state machine storage.
- Upon initiating a payment from user A to user B, both state machines would be loaded into memory.
- Payment from User A (`ActionInitInitiator` state change) would be dispatched to User A's state machine.
- Event from A's state machine (`SendLockedTransfer`) would not have to be sent over the wire but rather internally converted into a state change to go into B's state machine.
- APIs provided by your custom client would be generic multi-account capable APIs in this case.

Other capabilities and custom functionalities are also possible such as implementing paywalls, bots and payment services.

### How does Raiden.rs internally work?

Raiden.rs is implemented in Rust which emphasizes performance, type safety, and concurrency. It enforces memory safety—ensuring that all references point to valid memory—without requiring the use of a garbage collector or reference counting present in other memory-safe languages.

In addition, Raiden.rs is implemented to run asynchronosly which is powered by [Tokio](https://tokio.rs/)'s runtime under the hood. The tokio runtime handles running different asynchronous tasks in a multi-threaded runtime which enables Raiden to be mor scalable. However, in order to guarantee the absence of race-conditions, the Raiden client wraps the state machine in a single threaded locking library called [parking_lot](https://crates.io/crates/parking_lot) which means that asynchronous tasks across multiple threads that need to alter the state machine with state changes, can only do that as long as there is no other task holding the lock.

The `blockchain` crate mentioned previously, uses [`rust-web3`](https://github.com/tomusdrw/rust-web3/) which is also tokio-compatible and provided functionality to interact with the on-chain contracts to query data & submit transactions. This crate is heavily used in Raiden.rs.

## Usage

### Install Rust

Using rustup (The recommended way)

``` sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Clone the repo

``` sh
git clone https://github.com/rakanalh/raiden.rs.git
```

### Building Raiden client

``` sh
cd raiden.rs
cargo build --release -p raiden
```

Invoke the help command to see all available options:

``` sh
./target/release/raiden --help
```

### Running Raiden on a local test chain

``` sh
export Address="0x551a3Ac81ca6c1780f8cD378dB33373f259D7200"

./target/release/raiden
  --chain-id 4321
  --datadir /path/to/datadir
  --keystore-path /path/to/keystore
  --address $Address
  --password-file /path/to/password_file.txt
  --eth-rpc-endpoint http://localhost:8545
  --eth-rpc-socket-endpoint ws://localhost:8546
  --pathfinding-service-address http://localhost:6000
  --matrix-server http://localhost:8008
  --log-config debug
  --api-address 127.0.0.1:3000
  --default-settle-timeout 40
  --default-reveal-timeout 20
```
