//! Kanin-specific error types.

use std::convert::Infallible;

use log::{error, warn};
use prost::DecodeError;
use thiserror::Error as ThisError;

/// Errors that may be returned by `kanin`, especially when the app runs.
#[derive(Debug, ThisError)]
pub enum Error {
    /// The app was started with no handlers registered.
    #[error("No handlers were registered on the app.")]
    NoHandlers,
    /// An error from an underlying lapin call.
    #[error("An underlying `lapin` call failed: {0}")]
    Lapin(#[from] lapin::Error),
}

/// Errors that may be produced by handlers. Failing extractors provided by `kanin` return this error.
#[derive(Debug, ThisError)]
pub enum HandlerError {
    /// Errors due to invalid requests.
    #[error("Invalid Request: {0:#}")]
    InvalidRequest(#[from] RequestError),

    /// Internal server errors that are not the fault of clients.
    #[error("Internal Server Error: {0:#}")]
    Internal(#[from] ServerError),
}

impl HandlerError {
    /// Convenient const for this otherwise long error.
    pub const DELIVERY_ALREADY_EXTRACTED: Self =
        Self::Internal(ServerError::DeliveryAlreadyExtracted);
}

/// All the ways a request might be invalid.
#[derive(Debug, ThisError)]
pub enum RequestError {
    /// A message could not be decoded into the required type.
    ///
    /// This error is left as an opaque error as that is what is provided by [`prost`].
    #[error("Message could not be decoded into the required type: {0:#}")]
    DecodeError(#[from] DecodeError),
}

/// Errors due to bad configuration or usage from the server-side.
#[derive(Debug, ThisError)]
pub enum ServerError {
    /// A handler attempted to extract the delivery of a message twice.
    #[error("The delivery was already extracted from the request and could not be accessed")]
    DeliveryAlreadyExtracted,
    /// A handler attempted to extract a [`crate::extract::State`] but the state type had not been added to the app.
    #[error("The called handler was misconfigured. If you're the app owner, please see the logs for details")]
    StateNotFound,
}

/// Types that may be constructed from errors.
///
/// You must implement `FromError<kanin::HandlerError> for T` for any return type `T` of your handlers.
/// This is so that kanin knows how to construct an instance of your response from its internal errors.
///
/// If you want to implement this on a protobuf message, you can derive it easily using [`kanin::derive::ProstDeriveExt`].
pub trait FromError<Err> {
    /// Converts the error into a response.
    fn from_error(error: Err) -> Self;
}

/// This impl ensures that extractors that use `Infallible` as their error type will automatically "just work".
///
/// This will be unnecessary once `!` is stabilized, as `!` should automatically implement every appropriate trait.
impl<T> FromError<Infallible> for T {
    fn from_error(error: Infallible) -> Self {
        match error {}
    }
}

/// This impl ensures that if T can be constructed from an error, then Option<T> can also be constructed from an error.
/// Simply by wrapping in Some, obviously.
impl<T> FromError<HandlerError> for Option<T>
where
    T: FromError<HandlerError>,
{
    fn from_error(error: HandlerError) -> Self {
        Some(FromError::from_error(error))
    }
}

impl From<DecodeError> for HandlerError {
    fn from(e: DecodeError) -> Self {
        RequestError::from(e).into()
    }
}

// This implementation makes it so handlers can return (), in case they don't want to produce a response.
// In this case, since no response is given to the caller, we should log the error ourselves to make sure it is reported somehow.
impl FromError<HandlerError> for () {
    fn from_error(error: HandlerError) -> Self {
        match error {
            HandlerError::InvalidRequest(e) => {
                warn!("Listener handler received an invalid request: {e:#}")
            }
            HandlerError::Internal(e) => error!("Internal error on listener handler: {e:#}"),
        }
    }
}
