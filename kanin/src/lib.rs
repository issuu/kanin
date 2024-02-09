//! # kanin
//!
//! A framework for AMQP built on top of [lapin](https://github.com/amqp-rs/lapin).
//!
//! kanin makes it easy to create RPC microservices using protobuf in Rust with minimal boilerplate.
//!
//! # Example
//! ```no_run
//! # mod protobuf {
//! #     #[derive(kanin::FromError)]
//! #     #[derive(Clone, PartialEq, ::prost::Message)]
//! #     pub struct InvalidRequest {
//! #         #[prost(string, tag="1")]
//! #         pub error: ::prost::alloc::string::String,
//! #     }
//! #     #[derive(Clone, PartialEq, ::prost::Message)]
//! #     pub struct InternalError {
//! #         #[prost(string, tag="1")]
//! #         pub source: ::prost::alloc::string::String,
//! #         #[prost(string, tag="2")]
//! #         pub error: ::prost::alloc::string::String,
//! #     }
//! #     #[derive(Clone, PartialEq, ::prost::Message)]
//! #     pub struct EchoRequest {
//! #         #[prost(string, tag="1")]
//! #         pub value: ::prost::alloc::string::String,
//! #     }
//! #     #[derive(kanin::FromError)]
//! #     #[derive(Clone, PartialEq, ::prost::Message)]
//! #     pub struct EchoResponse {
//! #         #[prost(oneof="echo_response::Response", tags="1, 2, 3")]
//! #         pub response: ::core::option::Option<echo_response::Response>,
//! #     }
//! #     /// Nested message and enum types in `EchoResponse`.
//! #     pub mod echo_response {
//! #         #[derive(Clone, PartialEq, ::prost::Message)]
//! #         pub struct Success {
//! #             #[prost(string, tag="1")]
//! #             pub value: ::prost::alloc::string::String,
//! #         }
//! #         #[derive(kanin::FromError)]
//! #         #[derive(Clone, PartialEq, ::prost::Oneof)]
//! #         pub enum Response {
//! #             #[prost(message, tag="1")]
//! #             Success(Success),
//! #             #[prost(message, tag="2")]
//! #             InternalError(super::InternalError),
//! #             #[prost(message, tag="3")]
//! #             InvalidRequest(super::InvalidRequest),
//! #         }
//! #     }
//! #
//! #     impl EchoResponse {
//! #         pub fn success(value: String) -> Self {
//! #             Self { response: Some(echo_response::Response::Success(echo_response::Success { value })) }
//! #         }
//! #     }
//! # }
//! # use kanin::{extract::{Msg, State}, App, AppState};
//! # use protobuf::{EchoRequest, EchoResponse};
//! #
//! // EchoRequest and EchoResponse are protobuf messages as generated by prost_build: https://docs.rs/prost-build/0.10.4/prost_build/index.html
//! async fn echo(Msg(request): Msg<EchoRequest>, State(num): State<u8>) -> EchoResponse {
//!     assert_eq!(42, num);
//!
//!     EchoResponse::success(request.value)
//! }
//!
//! #[derive(AppState)]
//! struct MyState {
//!     num: u8
//! }
//!
//! #[tokio::main]
//! async fn main() -> kanin::Result<()> {
//!     App::new(MyState { num: 42 })
//!         .handler("my_routing_key", echo)
//!         .run("amqp://localhost")
//!         .await
//! }
//! ```
//!
//! # Help, why is my handler rejected by kanin?
//! There can be several reasons.
//!
//! Firstly, ensure that all parameters implement [`Extract`].
//! Especially for [`Msg`](extract::Msg), ensure the version of the `prost` crate used for the inner type is the same as the prost type used by `kanin`.
//! You can remove parameters one by one until the function is accepted to find out which parameter is the problem.
//! You can see if you have multiple `prost` versions by checking your `Cargo.lock` file.
//!
//! Secondly, ensure that the response type implements [`Respond`]. Once again, Protobuf messages automatically implement this but your `prost` version must match.
//!
//! If you're sure these things are handled, try to replace the body of the handler with `todo!()`.
//! If this causes the handler to work, then it's likely that the future your async function is creating is not [`Send`].
//! Your future must be [`Send`]. It is probably not [`Send`] because you're holding on to a type that is not [`Send`] across an await point.
//! For instance, holding a [`std::sync::MutexGuard`] across an await point will cause your future to not be [`Send`].

// kanin is 100% Safe Rust.
#![forbid(unsafe_code)]
#![warn(
    // Warns on ::path, allows crate::path.
    absolute_paths_not_starting_with_crate,

    // Warns you about missing documentation comments.
    // Writing documentation is a good idea! They will show up in your IDE as well.
    // Consider this a friendly nudge :)
    missing_docs,
    clippy::missing_docs_in_private_items,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,

    // Warns you when you have dependencies you're not using.
    unused_crate_dependencies,

    // Warns on converting values using the `as` keyword.
    // Converting in this way panics in case of errors. Consider using the `Into` or `TryInto` traits instead.
    clippy::as_conversions,
)]

// Re-exporting underlying lapin version so you don't have to add the same version as a dependency.
pub use lapin;
// Also re-exporting connection for easy access.
pub use lapin::Connection;

pub mod app;
pub mod error;
pub mod extract;
pub mod handler;
pub mod handler_config;
pub mod request;
pub mod response;

// pub-using every name::Name to avoid having to have kanin::name::Name repetition.
// This way you can just do kanin::Name.
pub use app::App;
pub use error::Error;
pub use error::HandlerError;
pub use extract::Extract;
pub use handler::Handler;
pub use handler_config::HandlerConfig;
pub use kanin_derive::AppState;
pub use kanin_derive::FromError;
pub use request::Request;
pub use response::Respond;

/// Convenience type for a result with `kanin`'s error.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    mod basic;
    mod send_recv;

    use std::time::Duration;

    use lapin::{Connection, ConnectionProperties};
    use tracing::warn;

    const TEST_AMQP_ADDR: &str = "amqp://localhost";

    /// Initializes test logging.
    fn init_logging() {
        std::env::set_var("RUST_LOG", "debug");
        let _ = env_logger::builder().is_test(true).try_init();
    }

    /// Returns a connection to AMQP. This will retry until a succesful connection is established.
    async fn amqp_connect() -> Connection {
        let mut attempts = 0;
        let conn = loop {
            match Connection::connect(TEST_AMQP_ADDR, ConnectionProperties::default()).await {
                Ok(conn) => break conn,
                Err(e) => {
                    warn!("Retrying connection");
                    attempts += 1;
                    if attempts > 8 {
                        panic!("Failed to establish a connection to AMQP. Ensure that a RabbitMQ instance is running on the 5672 port on localhost. Error: {e}")
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };

        conn.on_error(|e| {
            panic!("Connection returned error: {e:#}");
        });

        conn
    }
}
