//! AMQP requests.

use std::sync::Arc;

use lapin::options::{BasicAckOptions, BasicNackOptions};
use lapin::protocol::basic::AMQPProperties;

use lapin::{message::Delivery, Channel};
use tracing::{debug, error, warn};

use crate::extract::ReqId;

/// An AMQP request.
#[derive(Debug)]
pub struct Request<S> {
    /// The app state. This is added to the app at construction in [`crate::App::new`] and given to each request.
    state: Arc<S>,
    /// Request ID. This is a unique ID for every request. Either a newly created UUID or whatever
    /// is found in the `req_id` header of the incoming AMQP message.
    req_id: ReqId,
    /// Has this message been (n)ack'ed?
    acked: bool,
    /// The channel the message was received on.
    channel: Channel,
    /// The message delivery.
    delivery: Delivery,
}

impl<S> Request<S> {
    /// Constructs a new request from a [`Channel`] and [`Delivery`].
    pub fn new(channel: Channel, delivery: Delivery, state: Arc<S>) -> Self {
        Self {
            state,
            channel,
            acked: false,
            req_id: ReqId::from_delivery(&delivery),
            delivery,
        }
    }

    /// Returns a reference to the request ID of this request.
    pub fn req_id(&self) -> &ReqId {
        &self.req_id
    }

    /// Returns a reference to the delivery of this request.
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
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
    pub fn properties(&self) -> &AMQPProperties {
        &self.delivery.properties
    }

    /// Returns the `app_id` AMQP property of the request.
    pub fn app_id(&self) -> Option<&str> {
        self.properties()
            .app_id()
            .as_ref()
            .map(|app_id| app_id.as_str())
    }

    /// Acks the request, letting the AMQP broker know that it was received and processed successfully.
    pub(crate) async fn ack(&mut self, options: BasicAckOptions) -> Result<(), lapin::Error> {
        self.delivery.ack(options).await?;
        self.acked = true;
        Ok(())
    }
}

/// We implement [`Drop`] on [`Request`] to ensure that requests that were not explicitly acknowledged will be nacked.
impl<S> Drop for Request<S> {
    fn drop(&mut self) {
        // If we already acked, do nothing.
        if self.acked {
            return;
        }

        // We haven't acked and the request is being dropped.
        // This almost certainly indicates a panic during request handling.
        // We will nack the request to tell the AMQP broker to requeue this message ASAP.
        warn!("Nacking unacked request {} due to drop.", self.req_id);

        let req_id = self.req_id.clone();
        // Yoink the acker from the delivery so we can give it to a future to nack the message.
        let acker = std::mem::take(&mut self.delivery.acker);

        // Nacking is async so we have to spawn a task to do it.
        // Unfortunately we can't really be sure that this ever completes.
        tokio::spawn(async move {
            match acker
                .nack(BasicNackOptions {
                    multiple: false,
                    requeue: true,
                })
                .await
            {
                Ok(()) => debug!("Successfully nacked request {} during drop.", req_id),
                Err(e) => error!("Failed to nack request {} during drop: {e}", req_id),
            }
        });

        // Strictly speaking not necessary but nice to indicate that we have at least tried (even if we only try in the future).
        self.acked = true;
    }
}
