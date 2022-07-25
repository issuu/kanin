//! Types and utilities for the App's tokio tasks.

use std::{fmt, pin::Pin, sync::Arc};

use futures::{stream::FuturesUnordered, Future, StreamExt};
use lapin::{
    acker::Acker,
    options::{BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, Consumer,
};
use log::{debug, error, warn};

use crate::{Handler, QueueConfig, Request, Respond};

use super::StateMap;

/// The default exchange is indicated by the empty string in AMQP.
const DEFAULT_EXCHANGE: &str = "";

/// Handler tasks are the async functions that are run in the tokio tasks to perform handlers.
///
/// They use a given consumer and channel handle in order to receive AMQP deliveries.
/// The deliveries are then used to extract the required information according to the extractors of the handler.
///
/// Handler tasks should never return (they should keep processing messages),
/// but should they ever complete (perhaps RabbitMQ cancelled the consumer), then it returns the handler's routing key.
pub(super) type HandlerTask = Pin<Box<dyn Future<Output = String> + Send>>;

/// Creates the handler task for the given handler and routing key. See [`HandlerTask`].
fn handler_task<H, Args, Res>(
    routing_key: String,
    handler: H,
    channel: Channel,
    mut consumer: Consumer,
    state: Arc<StateMap>,
) -> HandlerTask
where
    H: Handler<Args, Res> + Send + 'static,
    Res: Respond + fmt::Debug + Send,
{
    Box::pin(async move {
        // We keep a set of handles to all outstanding spawned tasks.
        let mut tasks = FuturesUnordered::new();

        // We keep listening for requests from the consumer until the consumer cancels.
        // See lapin::Consumer's implementation of Stream.
        loop {
            let delivery = tokio::select! {
                // Listen on new deliveries.
                delivery = consumer.next() => match delivery {
                    // Received a delivery succesfully, just unwrap it from the option.
                    Some(delivery) => delivery,
                    // We should only ever get to this point if the consumer is cancelled.
                    // We'll just return the routing key - might be a help for the user to see which
                    // routing key got cancelled.
                    None => return routing_key,
                },
                // Check return values of previously spawned handlers.
                Some(result) = tasks.next() => if let Err(e) = result {
                    panic!("Request handler for \"{routing_key}\" panicked: {e:#}");
                } else {
                    // If the inner result is not an error, we just ignore it,
                    // it's just a request that finished handling in that case.
                    continue;
                },
            };

            let req = match delivery {
                Err(e) => {
                    error!("Error when receiving delivery on routing key \"{routing_key}\": {e:#}");
                    continue;
                }
                // Construct the request by bundling the channel and the delivery.
                Ok(delivery) => Request::new(channel.clone(), delivery, state.clone()),
            };

            // Now handle the request.
            let handler = handler.clone();
            let channel = channel.clone();
            // Requests are handled and replied to concurrently.
            // This allows each handler task to process multiple requests at once.
            tasks.push(tokio::spawn(async move {
                handle_request(req, handler, channel).await;
            }));
        }
    })
}

/// Handles the given request with the given handler and channel.
///
/// Acks the request and responds with the given acker as appropriate.
async fn handle_request<H, Args, Res>(mut req: Request, handler: H, channel: Channel)
where
    H: Handler<Args, Res> + Send + 'static,
    Res: Respond + fmt::Debug + Send,
{
    let reply_to = req.reply_to().cloned();
    let correlation_id = req.correlation_id().cloned();
    // Call the handler with the request.
    let response = handler.call(&mut req).await;
    debug!(
        "Handler {:?} produced response: {response:?}",
        std::any::type_name::<H>()
    );

    let bytes_response = response.respond();

    // If we were given a way to reply, use it to reply.
    // Note that if the request did not contain a reply_to, we don't even try to reply (how would we?).
    if let Some(reply_to) = reply_to {
        let mut props = BasicProperties::default();

        if let Some(correlation_id) = correlation_id {
            props = props.with_correlation_id(correlation_id);
        } else {
            warn!("Request for handler {:?} did not contain a `correlation_id` property. A reply will be published, but the receiver may not recognize it as the reply for their request.", std::any::type_name::<H>());
        }

        // Warn in case of replying with an empty message, since this is _probably_ wrong or unintended.
        if bytes_response.is_empty() {
            warn!("Handler {:?} produced an empty response to a message with a `reply_to` property. This is probably undesired, as the caller likely expects more of a response.", std::any::type_name::<H>());
        }

        let publish = channel
            .basic_publish(
                DEFAULT_EXCHANGE,
                reply_to.as_str(),
                BasicPublishOptions::default(),
                &bytes_response,
                props,
            )
            .await;

        match publish {
            Ok(_confirm) => {
                debug!("Successfully published reply to routing key \"{reply_to}\"");
            }
            // We tried to reply but somehow our response never got published.
            // We'll log an error in this case. Panicking probably doesn't help much.
            Err(e) => {
                error!("Error when publishing reply to routing key \"{reply_to}\": {e:#}");
            }
        }
    } else if !bytes_response.is_empty() {
        // We only warn if the response is not empty.
        // Empty responses may be produced by non-responding handlers, which is fine.
        warn!("Received message for handler {:?} but the request did not contain a `reply_to` property, so no reply could be published.", std::any::type_name::<H>());
    };

    match req.delivery.map(|d| d.acker) {
        // Check if it's the default - this signifies that it was already extracted.
        // In that case, it is the responsibility of the handler to acknowledge, so we won't do it.
        Some(acker) if acker != Acker::default() => {
            match acker.ack(BasicAckOptions::default()).await {
                Ok(()) => debug!("Successfully acked request."),
                Err(e) => error!("Failed to ack request: {e:#}"),
            }
        }
        // If the delivery or acker was extracted, it is up to the request handler itself to acknowledge the request.
        _ => (),
    }
}

/// Task factories take a channel, consumer and the app state and produces a task for running in tokio.
///
/// This type is saved by [`App`] during calls to [`App::handler`][crate::App::handler].
/// It is how the [`App`] keeps the handlers saved before running.
///
/// Upon calling [`App::run`][crate::App::run], channels and consumers are created for each task factory,
/// creating a [`HandlerTask`] which can then be run in tokio.
///
/// In a nutshell:
/// 1. User creates handler function.
/// 2. User calls [`App::handler`][crate::App::handler], saving the handler as a `TaskFactory`.
/// 3. User calls [`App::run`][crate::App::run], creating tasks from all the task factories that are then run in tokio.
///
/// [`App`]: crate::App
pub(super) struct TaskFactory {
    /// The routing key of the handler task produced by this task factory.
    routing_key: String,
    /// Queue configuration for the handler task produced by this task factory.
    queue_config: QueueConfig,
    /// The factory function that constructs the handler task from the given channel, consumer and state map.
    factory: Box<dyn FnOnce(Channel, Consumer, Arc<StateMap>) -> HandlerTask + Send>,
}

impl TaskFactory {
    /// The in-built direct exchange.
    const DIRECT_EXCHANGE: &'static str = "amq.direct";

    /// Constructs a new task factory from the given routing key and handler.
    pub(super) fn new<H, Args, Res>(
        routing_key: String,
        handler: H,
        queue_config: QueueConfig,
    ) -> Self
    where
        H: Handler<Args, Res> + Send + 'static,
        Res: Respond + fmt::Debug + Send,
    {
        // A task factory is a closure in a box that produces a handler task.
        Self {
            routing_key: routing_key.clone(),
            queue_config,
            factory: Box::new(
                move |channel: Channel, consumer: Consumer, state: Arc<StateMap>| {
                    handler_task(routing_key, handler, channel, consumer, state)
                },
            ),
        }
    }

    /// Retrieves the routing key for this task factory.
    pub(super) fn routing_key(&self) -> &str {
        &self.routing_key
    }

    /// Builds the task, returning a [`HandlerTask`].
    pub(super) async fn build(
        self,
        conn: &Connection,
        state_map: Arc<StateMap>,
    ) -> lapin::Result<HandlerTask> {
        // Create the dedicated channel for this handler.
        let channel = conn.create_channel().await?;

        // Set prefetch according to the desired configuration.
        channel
            .basic_qos(self.queue_config.prefetch, BasicQosOptions::default())
            .await?;

        // Declare and bind the queue, as specified by AMQP.
        channel
            .queue_declare(
                &self.routing_key,
                self.queue_config.options,
                self.queue_config.arguments,
            )
            .await?;
        channel
            .queue_bind(
                &self.routing_key,
                Self::DIRECT_EXCHANGE,
                &self.routing_key,
                Default::default(),
                Default::default(),
            )
            .await?;

        let consumer = channel
            .basic_consume(
                &self.routing_key,
                &self.routing_key,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok((self.factory)(channel, consumer, state_map))
    }
}
