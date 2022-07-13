# 🐰 kanin
<a href="https://crates.io/crates/kanin">
    <img src="https://img.shields.io/crates/v/kanin.svg" alt="Crates.io version" />
</a>
<a href="https://docs.rs/kanin">
    <img src="https://img.shields.io/docsrs/kanin" alt="Documentation status" />
</a>

A framework for AMQP built on top of [lapin](https://github.com/amqp-rs/lapin) that makes it easy to create RPC microservices in Rust 🦀, using Protobuf.

# Usage
Run `cargo add kanin` to add kanin to your Cargo.toml.

```rust
mod protobuf;

use kanin::{extract::Msg, App};
use protobuf::echo::{EchoRequest, EchoResponse};

async fn echo(Msg(request): Msg<EchoRequest>) -> EchoResponse {
    EchoResponse::success(request.value)
}

#[tokio::main]
async fn main() -> kanin::Result<()> {
    App::new()
        .handler("my_routing_key", echo)
        .run("amqp_addr")
        .await
}
```

See the documentation for examples and more specific usage, or see the `/minimal_example` folder in this repo.

# Testing
To run tests, install [just](https://github.com/casey/just) and [Docker](https://www.docker.com/) (you need docker-compose).

Then, simply run `just test`, which will launch a RabbitMQ instance in a container that the tests will connect to.
