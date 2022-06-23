//! # kanin
//!
//! A framework for AMQP built on top of [lapin](https://github.com/amqp-rs/lapin).
//!
//! kanin makes it easy to create RPC microservices using protobuf in Rust with minimal boilerplate.

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

pub mod app;
pub mod error;
pub mod extract;
pub mod handler;
pub mod queue_config;
pub mod request;
pub mod response;

// pub-using every name::Name to avoid having to have kanin::name::Name repetition.
// This way you can just do kanin::Name.
pub use app::App;
pub use error::Error;
pub use error::HandlerError;
pub use extract::Extract;
pub use handler::Handler;
pub use kanin_derive::FromError;
pub use queue_config::QueueConfig;
pub use request::Request;
pub use response::Respond;

/// Convenience type for a result with `kanin`'s error.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    mod send_recv;

    use std::time::Duration;

    use lapin::{Connection, ConnectionProperties};
    use log::warn;

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
