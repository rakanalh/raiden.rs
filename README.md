# raiden-rust
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


## Building

``` sh
cargo build --release
```

## Running

``` sh
./target/release/raiden --chain-id goerli --eth-rpc-endpoint {JSON_RPC_ENDPOINT} --eth-rpc-socket-endpoint {WSS_ENDPOINT} --keystore-path {ETH_KEYSTORE_PATH}
```

For the time being, i am testing on `Goerli` network with Infura where the JSON RPC endpoint looks like: `https://goerli.infura.io/v3/{ID}` and Websocket endpoint looks like: `wss://goerli.infura.io/ws/v3/{ID} `. For geth, i am guessing that you have to use `geth --goerli --syncmode full --http --ws` where geth provides the URIs for http and websocket addresses.

## TODO
- [x] Chain Sync Service
- [ ] State machines
  - [ ] Chain 
  - [ ] Token Network
  - [ ] Channel
- [ ] Contract Proxies (WIP)
- [ ] Networking
  - [ ] Matrix
  - [ ] WebRTC?
- [ ] Transfer Logic
- [ ] HTTP API
- [ ] PFS / MS
- [ ] Documentation
