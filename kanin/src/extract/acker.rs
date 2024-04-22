//! Manual acknowledgement and rejection.

use std::mem;

use async_trait::async_trait;
use lapin::{
    acker::Acker as LapinAcker,
    options::{BasicAckOptions, BasicRejectOptions},
};

use crate::{Extract, HandlerError, Request};

/// An extractor that allows you manual control of acknowledgement and rejection of messages.
///
/// Note that when you extract an `Acker`, kanin _will not_ acknowledge the message for you.
/// Neither will it reject the message if your handler panicks.
///
/// When you extract this, you are responsible for acknowledging or rejecting yourself.
#[must_use = "You must call .ack or .reject in order to acknowledge or reject the message."]
#[derive(Debug)]
pub struct Acker(LapinAcker);

impl Acker {
    /// Acks the message that was received for this acker.
    ///
    /// # Errors
    /// Returns `Err` on network failures.
    // Note that since we consume the acker, it should not be possible to call this twice.
    // Thus that error possibility is not listed.
    pub async fn ack(self) -> Result<(), lapin::Error> {
        self.0
            .ack(BasicAckOptions {
                // It does not make sense to use this flag with kanin, as it might interfere with handling of other previous messages.
                multiple: false,
            })
            .await
    }

    /// Rejects the message that was received for this acker.
    ///
    /// # Errors
    /// Returns `Err` on network failures.
    // Note that since we consume the acker, it should not be possible to call this twice.
    // Thus that error possibility is not listed.
    pub async fn reject(self, options: BasicRejectOptions) -> Result<(), lapin::Error> {
        self.0.reject(options).await
    }
}

/// Extract implementation for the AMQP acker.
#[async_trait]
impl<S> Extract<S> for Acker
where
    S: Send + Sync,
{
    type Error = HandlerError;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        // This is quite a hacky way of taking the acker. We should improve this if/when lapin improves the interface.
        // See also https://github.com/amqp-rs/lapin/issues/402.
        let acker = mem::take(&mut req.delivery_mut().acker);

        // The request will consider itself acked. It is up to the handler to actually ack the request.
        req.acked = true;

        if acker == LapinAcker::default() {
            panic!(
                "extracted acker was equal to the default acker - did you extract an acker twice?"
            );
        }

        Ok(Acker(acker))
    }
}
