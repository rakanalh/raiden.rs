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

