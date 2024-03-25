//! Super minimal example that just sets up an app with a simple handler.

mod protobuf;

use kanin::{extract::Msg, App};
use protobuf::echo::{EchoRequest, EchoResponse};

async fn echo(Msg(request): Msg<EchoRequest>) -> EchoResponse {
    EchoResponse::success(request.value)
}

#[tokio::main]
async fn main() -> kanin::Result<()> {
    App::new(())
        .handler("my_routing_key", echo)
        .graceful_shutdown_on_signal()
        .run("amqp_addr")
        .await
}
