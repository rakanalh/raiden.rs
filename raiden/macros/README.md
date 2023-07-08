## Raiden Macros

Defines some derive macros for converting events and state changes.

### IntoStateChange

``` rust
#[derive(IntoStateChange)]
pub struct Block {
    ...snip
}
```

### IntoEvent

Implemented for events.

``` rust
#[derive(IntoEvent)]
pub struct SendWithdrawRequest {
    ...snip
}
```
