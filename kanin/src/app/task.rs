//! Types and utilities for the App's tokio tasks.

use std::{any::type_name, pin::Pin, sync::Arc, time::Instant};

use futures::{stream::FuturesUnordered, Future, StreamExt};
use lapin::{
    options::{
        BasicAckOptions, BasicCancelOptions, BasicConsumeOptions, BasicPublishOptions,
        BasicQosOptions,
    },
    types::{FieldTable, ShortString},
    BasicProperties, Channel, Connection, Consumer,
};
use metrics::gauge;
use tokio::sync::broadcast;
use tracing::{debug, error, error_span, info, trace, warn, Instrument};

use crate::{Error, Handler, HandlerConfig, Request, Respond, Result};

/// Handler tasks are the async functions that are run in the tokio tasks to perform handlers.
///
/// They use a given consumer and channel handle in order to receive AMQP deliveries.
/// The deliveries are then used to extract the required information according to the extractors of the handler.
///
/// Handler tasks should never return unless the app is instructed to shut down.
type HandlerTask = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// Handler task factories are functions that produce handler tasks by providing all the necessary components the handler tasks need.
///
/// Upon creating an app and registering handlers, factories are inserted into the app. It is only upon running the app that the
/// factories are turned into actual handler tasks and run in the asynchronous runtime.
type HandlerTaskFactory<S> =
    Box<dyn FnOnce(Channel, Consumer, f64, Arc<S>, broadcast::Receiver<()>) -> HandlerTask + Send>;

/// Creates the handler task for the given handler and routing key. See [`HandlerTask`].
#[allow(clippy::too_many_arguments)]
fn handler_task<H, S, Args, Res>(
    routing_key: String,
    handler: H,
    channel: Channel,
    mut consumer: Consumer,
    prefetch: f64,
    state: Arc<S>,
    mut shutdown: broadcast::Receiver<()>,
    should_reply: bool,
) -> HandlerTask
where
    H: Handler<Args, Res, S>,
    Res: Respond,
    S: Send + Sync + 'static,
{
    Box::pin(async move {
        // We keep a set of handles to all outstanding spawned tasks.
        let mut tasks = FuturesUnordered::new();

        // We keep listening for requests from the consumer until the consumer cancels or we're instructed to shut down.
        let ret = loop {
            let delivery = tokio::select! {
                // "Biased" here means that instead of randomly selecting a path, Tokio will check from top to bottom.
                // This ensures that we check for shutdown before receiving a new message.
                // It also means that we prioritize emptying the already-started handlers before spawning new handlers.
                biased;

                // Check if we need to shut down.
                _ = shutdown.recv() => {
                    info!("Graceful shutdown signal received in handler {}.", type_name::<H>());
                    // Break out of the loop with no error. No error indicates a graceful shutdown.
                    break Ok(())
                }

                // Check return values of previously spawned handlers.
                Some(result) = tasks.next() => if let Err(e) = result {
                    // A handler panicked. We won't shut down the whole system in this case, we'll just continue with the next call.
                    // The hope is that the panic is a temporary thing.
                    error!("Handler {} panicked: {}", type_name::<H>().to_string(), e);
                    continue
                } else {
                    // If the inner result is not an error, we just ignore it,
                    // it's just a request that finished handling in that case.
                    continue;
                },

                // Listen on new deliveries.
                delivery = consumer.next() => match delivery {
                    // Received a delivery successfully, just unwrap it from the option.
                    Some(delivery) => delivery,

                    // We should only ever get to this point if the consumer is cancelled (see lapin::Consumer's implementation of Stream).
                    // We'll attempt a graceful shutdown in this case.
                    // We'll return the routing key - might be a help for the user to see which consumer got cancelled.
                    None => {
                        error!("Consumer cancelled, attempting to gracefully shut down...");
                        break Err(Error::ConsumerCancelled(routing_key));
                    },
                },
            };

            let req = match delivery {
                Err(e) => {
                    error!("Error when receiving delivery on routing key \"{routing_key}\": {e:#}");
                    continue;
                }
                // Construct the request by bundling the channel, the delivery and the app state.
                Ok(delivery) => Request::new(channel.clone(), delivery, state.clone()),
            };

            // Now handle the request.
            let handler = handler.clone();
            let channel = channel.clone();
            // Requests are handled and replied to concurrently.
            // This allows each handler task to process multiple requests at once.
            tasks.push(tokio::spawn(async move {
                let span = error_span!("request", req_id = %req.req_id());

                handle_request(req, handler, channel, should_reply)
                    .instrument(span)
                    .await;
            }));
        };

        // We won't process any further requests, so we'll cancel the consumer.
        let queue = consumer.queue();
        let consumer_tag = consumer.tag();
        let tag = consumer_tag.as_str();

        if let Err(e) = channel
            .basic_cancel(tag, BasicCancelOptions::default())
            .await
        {
            error!("Failed to cancel consumer with tag {tag} and queue {queue} during graceful shutdown of handler task {} (graceful shutdown will continue regardless): {e}", type_name::<H>())
        }

        // We'll update the prefetch capacity gauge here.
        // That means that if this queue takes a long time to shut down,
        // it won't still appear as if it has capacity for many messages.
        gauge!("kanin.prefetch_capacity", "queue" => queue.to_string()).decrement(prefetch);

        if tasks.is_empty() {
            info!("No outstanding messages on handler {}.", type_name::<H>())
        } else {
            info!(
                "Handler {} finishing {} requests...",
                type_name::<H>(),
                tasks.len()
            );

            // Wait for the outstanding tasks to finish.
            let start = Instant::now();
            while let Some(res) = tasks.next().await {
                if let Err(e) = res {
                    error!(
                        "Handler {} panicked during graceful shutdown (graceful shutdown will continue): {}",
                        type_name::<H>().to_string(),
                        e
                    );
                }

                if !tasks.is_empty() {
                    info!(
                        "Handler {} still working on {} requests ({:?})...",
                        type_name::<H>(),
                        tasks.len(),
                        start.elapsed(),
                    )
                }
            }
            info!(
                "Handler {} finished in {:?}.",
                type_name::<H>(),
                start.elapsed(),
            )
        }

        ret
    })
}

/// Handles the given request with the given handler and channel.
///
/// Acks the request and responds if the handler executes normally.
///
/// If the handler panicks, the request will be rejected and instructed to requeue.
async fn handle_request<H, S, Args, Res>(
    mut req: Request<S>,
    handler: H,
    channel: Channel,
    should_reply: bool,
) where
    H: Handler<Args, Res, S>,
    Res: Respond,
{
    let handler_name = std::any::type_name::<H>();
    let app_id = req.app_id().unwrap_or("<unknown>");
    info!("Received request on handler {handler_name:?} from {app_id}");

    if req.delivery().redelivered {
        info!("Request was redelivered.");
    }

    let t = std::time::Instant::now();

    // Call the handler with the request.
    let response = handler.call(&mut req).await;

    let properties = req.properties();
    let reply_to = properties.reply_to();
    let correlation_id = properties.correlation_id();

    debug!("Handler {handler_name:?} produced response {response:?}");

    let bytes_response = response.respond();

    // Includes time for decoding request and encoding response, but *not* the time to publish the response.
    let elapsed = t.elapsed();

    match (should_reply, reply_to) {
        // We're supposed to reply and we have a reply_to queue: Reply.
        (true, Some(reply_to)) => {
            let mut props = BasicProperties::default();

            if let Some(correlation_id) = correlation_id {
                props = props.with_correlation_id(correlation_id.clone());
            } else {
                warn!("Request from handler {handler_name:?} did not contain a `correlation_id` property. A reply will be published, but the receiver may not recognize it as the reply for their request. (all properties: {properties:?})");
            }

            // Warn in case of replying with an empty message, since this is _probably_ wrong or unintended.
            if bytes_response.is_empty() {
                warn!("Handler {handler_name:?} produced an empty response to a message with a `reply_to` property. This is probably undesired, as the caller likely expects more of a response (elapsed={elapsed:?})");
            } else {
                info!(
                    "Response with {} bytes that will be published to {reply_to} (elapsed={elapsed:?})",
                    bytes_response.len()
                );
            }

            // Since we expect the response to be encoded Protobuf, we set the content type to octet-stream.
            props = props.with_content_type(ShortString::from("application/octet-stream"));

            let publish = channel
                .basic_publish(
                    HandlerConfig::DEFAULT_EXCHANGE,
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
        }
        // We are supposed to reply, but the request did not have a reply_to.
        // Even worse, the response we produced is non-empty - it was probably meant to be received by someone!
        // In this case, we warn. Empty responses may be produced by non-responding handlers, which is fine.
        (true, None) if !bytes_response.is_empty() => {
            warn!("Received non-empty message from handler {handler_name:?} but the request did not contain a `reply_to` property, so no reply could be published (all properties: {properties:?}, elapsed={elapsed:?}).");
        }
        // We are supposed to reply, but the request did not have a reply_to.
        // However we produced an empty response, so it's not like the caller missed any information.
        (true, None) => {
            info!(
                "Handler {handler_name} finished (empty, should_reply = true, elapsed={elapsed:?})",
            );
        }
        // We are not supposed to reply so we won't.
        (false, _) => {
            let len = bytes_response.len();
            info!(
                "Handler {handler_name} finished ({len} bytes, should_reply = false, elapsed={elapsed:?}).",
            );
        }
    };

    // Remember to ack, otherwise the AMQP broker will think we failed to process the request!
    // We don't ack if we've already done it, via the handler extracting the acker.
    if !req.acked {
        match req.ack(BasicAckOptions::default()).await {
            Ok(()) => debug!("Successfully acked request."),
            Err(e) => error!("Failed to ack request: {e:#}"),
        }
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
pub(super) struct TaskFactory<S> {
    /// The routing key of the handler task produced by this task factory.
    routing_key: String,
    /// Configuration for the handler task produced by this task factory.
    config: HandlerConfig,
    /// The factory function that constructs the handler task from the given channel, consumer and state.
    factory: HandlerTaskFactory<S>,
}

impl<S> TaskFactory<S> {
    /// Constructs a new task factory from the given routing key and handler.
    pub(super) fn new<H, Args, Res>(routing_key: String, handler: H, config: HandlerConfig) -> Self
    where
        H: Handler<Args, Res, S>,
        Res: Respond,
        S: Send + Sync + 'static,
    {
        let should_reply = config.should_reply;

        // A task factory is a closure in a box that produces a handler task.
        Self {
            routing_key: routing_key.clone(),
            config,
            factory: Box::new(
                move |channel: Channel,
                      consumer: Consumer,
                      prefetch: f64,
                      state: Arc<S>,
                      shutdown: broadcast::Receiver<()>| {
                    handler_task(
                        routing_key,
                        handler,
                        channel,
                        consumer,
                        prefetch,
                        state,
                        shutdown,
                        should_reply,
                    )
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
        state: Arc<S>,
        shutdown: broadcast::Receiver<()>,
    ) -> lapin::Result<HandlerTask> {
        debug!(
            "Building task for handler on routing key {:?}",
            self.routing_key(),
        );

        // Create the dedicated channel for this handler.
        trace!("Creating channel for handler...");
        let channel = conn.create_channel().await?;

        // Set prefetch according to the desired configuration.
        trace!(
            "Reporting basic quality of service with prefetch {}...",
            self.config.prefetch
        );
        channel
            .basic_qos(self.config.prefetch, BasicQosOptions::default())
            .await?;

        // If no queue was specified, we just use the routing key.
        let queue_name = self.config.queue.as_deref().unwrap_or(&self.routing_key);

        // Set prefetch capacity gauge according to the prefetch.
        // This allows one to construct a metric that informs how close a queue is to capacity.
        // I.e. if there are 3 servers with prefetch 8 on a queue, the queue's capacity is 24.
        // By comparing this number to the number of unacked messages in the AMQP message broker (like the rabbitmq_queue_messages_unacked metric from RabbitMQ),
        // you can estimate how close to capacity the queue is.
        let prefetch_f64: f64 = self.config.prefetch.into();
        gauge!("kanin.prefetch_capacity", "queue" => queue_name.to_string())
            .increment(prefetch_f64);

        // Declare and bind the queue. AMQP states that we must do this before creating the consumer.
        trace!("Declaring queue {queue_name:?} prior to binding...");
        channel
            .queue_declare(queue_name, self.config.options, self.config.arguments)
            .await?;

        trace!(
            "Binding to queue {queue_name:?} on exchange {:?} on routing key {:?}...",
            self.config.exchange,
            self.routing_key
        );
        channel
            .queue_bind(
                queue_name,
                &self.config.exchange,
                &self.routing_key,
                Default::default(),
                Default::default(),
            )
            .await?;

        trace!("Creating consumer on routing key {}...", self.routing_key);
        let consumer = channel
            .basic_consume(
                queue_name,
                &self.routing_key,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok((self.factory)(
            channel,
            consumer,
            prefetch_f64,
            state,
            shutdown,
        ))
    }
}
