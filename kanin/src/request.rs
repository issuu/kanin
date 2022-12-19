//! AMQP requests.

use std::sync::Arc;

use lapin::protocol::basic::AMQPProperties;

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

    /// Returns the AMQP properties of the request, unless the request was already extracted.
    pub fn properties(&self) -> Option<&AMQPProperties> {
        self.delivery.as_ref().map(|d| &d.properties)
    }
}
