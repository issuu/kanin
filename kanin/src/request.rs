//! AMQP requests.

use std::sync::Arc;

use lapin::protocol::basic::AMQPProperties;

use lapin::{message::Delivery, Channel};

use crate::extract::ReqId;

/// An AMQP request.
#[derive(Debug)]
pub struct Request<S> {
    /// The app state. This is added to the app at construction in [`crate::App::new`] and given to each request.
    state: Arc<S>,
    /// The channel the message was received on.
    channel: Channel,
    /// Request ID. This is a unique ID for every request. Either a newly created UUID or whatever
    /// is found in the `req_id` header of the incoming AMQP message.
    pub(crate) req_id: ReqId,
    /// The message delivery.
    pub(crate) delivery: Option<Delivery>,
}

impl<S> Request<S> {
    /// Constructs a new request from a [`Channel`] and [`Delivery`].
    pub fn new(channel: Channel, delivery: Delivery, state: Arc<S>) -> Self {
        Self {
            state,
            channel,
            req_id: ReqId::from_delivery(&delivery),
            delivery: Some(delivery),
        }
    }

    /// Returns the app state for the given type.
    pub fn state<T>(&self) -> T
    where
        T: for<'a> From<&'a S>,
    {
        self.state.as_ref().into()
    }

    /// Returns a reference to the [`Channel`] the message was delivered on.
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    /// Returns the AMQP properties of the request, unless the request was already extracted.
    pub fn properties(&self) -> Option<&AMQPProperties> {
        self.delivery.as_ref().map(|d| &d.properties)
    }

    /// Returns the `app_id` AMQP property of the request.
    pub fn app_id(&self) -> Option<&str> {
        self.properties()
            .and_then(|p| p.app_id().as_ref())
            .map(|app_id| app_id.as_str())
    }
}
