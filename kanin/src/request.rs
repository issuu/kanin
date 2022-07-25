//! AMQP requests.

use std::sync::Arc;

use lapin::types::ShortString;

use lapin::{message::Delivery, Channel};

use crate::app::StateMap;

/// An AMQP request.
#[derive(Debug)]
pub struct Request {
    /// The app state. This is added to the app through [`crate::App::state`] and given to each request.
    state: Arc<StateMap>,
    /// The channel the message was received on.
    channel: Channel,
    /// The message delivery.
    pub(crate) delivery: Option<Delivery>,
}

impl Request {
    /// Constructs a new request from a [`Channel`] and [`Delivery`].
    pub fn new(channel: Channel, delivery: Delivery, state: Arc<StateMap>) -> Self {
        Self {
            state,
            channel,
            delivery: Some(delivery),
        }
    }

    /// Returns the app state for the given type.
    /// Returns `None` if the app state has not been added to the app.
    pub fn state<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.state.get()
    }

    /// Returns a reference to the [`Channel`] the message was delivered on.
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    /// Returns the queue that reply messages should be sent to.
    ///
    /// If `None`, the request cannot be replied to.
    /// This is sometimes used intentionally, for example when handling events that require no response.
    pub fn reply_to(&self) -> Option<&ShortString> {
        self.delivery
            .as_ref()
            .and_then(|d| d.properties.reply_to().as_ref())
    }

    /// Returns the request's correlation ID.
    ///
    /// The correlation ID is put on the header of the response.
    /// The caller can then inspect the [`Request::reply_to`] queue for a response
    /// with a correlation ID that matches the request, thus *correlating*
    /// requests and responses.
    pub fn correlation_id(&self) -> Option<&ShortString> {
        self.delivery
            .as_ref()
            .and_then(|d| d.properties.correlation_id().as_ref())
    }
}
