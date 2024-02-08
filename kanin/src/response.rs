//! AMQP responses.
//!
//! Any type that implements [`Respond`] can be used as the return type of a handler.

use std::fmt;

use prost::Message;

/// A trait for types that may produce responses.
///
/// This really just means they can be converted into a byte-stream.
/// However, the type must also be able to be displayed for debugging purposes
/// and be sent across threads during processing.
pub trait Respond: fmt::Debug + Send {
    /// Creates the bytes payload of the response.
    fn respond(self) -> Vec<u8>;
}

/// This impl ensures that protobuf messages can be used as the return type of handlers.
impl<D: Message> Respond for D {
    fn respond(self) -> Vec<u8> {
        self.encode_to_vec()
    }
}
