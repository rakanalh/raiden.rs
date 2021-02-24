# raiden-rust
The unofficial Raiden client implementation in Rust

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
- [ ] Contract Proxies
- [ ] Networking
  - [ ] Matrix
  - [ ] WebRTC?
- [ ] Transfer Logic
- [ ] HTTP API
- [ ] PFS / MS
- [ ] Documentation
