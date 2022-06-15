//! AMQP responses.

use prost::Message;

/// A trait for types that may produce responses.
///
/// This really just means they can be converted into a byte-stream.
pub trait Respond {
    /// Creates the bytes payload of this value.
    fn respond(self) -> Vec<u8>;
}

/// This impl ensures that protobuf messages can be used as the return type of handlers.
impl<D: Message> Respond for D {
    fn respond(self) -> Vec<u8> {
        self.encode_to_vec()
    }
}
