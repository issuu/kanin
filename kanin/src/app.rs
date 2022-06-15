//! Module for the [App] struct and surrounding utilities.

mod task;

use std::{any::Any, fmt, sync::Arc};

use anymap::Map;
use futures::future::select_all;
use lapin::{self, Connection, ConnectionProperties};
use log::{debug, info, trace};

use self::task::TaskFactory;
use crate::{extract::State, Error, Handler, QueueConfig, Respond, Result};

/// Apps can hold state that can be extracted in handlers. This state is stored in a type-map.
pub(crate) type StateMap = Map<dyn Any + Send + Sync>;

/// The central struct of your application.
#[must_use = "The app will not do anything unless you call `.run`."]
pub struct App {
    /// A map from routing keys to task factories.
    /// Task factories are constructed in [`App::handler`] and called in [`App::run`].
    handlers: Vec<TaskFactory>,
    /// A map from types to a single value of that type.
    /// This is used to hold the state values that users may want to store before running the app,
    /// and then extract in their handlers.
    state: StateMap,
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: Map::new(),
            handlers: Vec::default(),
        }
    }
}

impl App {
    /// Creates a new kanin app with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new handler for the given routing key with the default prefetch count.
    ///
    /// The handler will respond to any messages with `reply_to` and `correlation_id` properties.
    /// This requires that the response type implements Respond (which is automatically implemented for protobuf messages).
    pub fn handler<H, Args, Res>(self, routing_key: impl Into<String>, handler: H) -> Self
    where
        H: Handler<Args, Res> + Send + 'static,
        Res: Respond + fmt::Debug + Send,
    {
        self.handler_with_config(routing_key, handler, Default::default())
    }

    /// Registers a new handler for the given routing key with the given queue configuration.
    ///
    /// The handler will respond to any messages with `reply_to` and `correlation_id` properties.
    /// This requires that the response type implements Respond (which is automatically implemented for protobuf messages).
    pub fn handler_with_config<H, Args, Res>(
        mut self,
        routing_key: impl Into<String>,
        handler: H,
        queue_config: QueueConfig,
    ) -> Self
    where
        H: Handler<Args, Res> + Send + 'static,
        Res: Respond + fmt::Debug + Send,
    {
        // Create and save the task factory - this is a function that creates the async task that will be run in tokio.
        self.handlers
            .push(TaskFactory::new(routing_key.into(), handler, queue_config));

        self
    }

    /// Adds a type as state to this app.
    ///
    /// An `App` may use any number of types as state. The app will contain one instance of each type.
    ///
    /// The state added to the app through this method can subsequently be used in request handlers,
    /// by making use of the [`crate::extract::State`] extractor.
    ///
    /// # Panics
    /// Panics if the given type has already been registered with the app.
    pub fn state<T: Send + Sync + 'static>(mut self, value: T) -> Self {
        if self.state.insert(State(Arc::new(value))).is_some() {
            panic!(
                "Attempted to register a state type, `{}` that had already been registered before! \
                You can only register one value of each type. If you need multiple values of the same type, \
                use the newtype pattern to signify the semantic difference between the two values.",
                std::any::type_name::<T>()
            );
        }
        self
    }

    /// Runs the app with all the handlers that have been registered.
    ///
    /// The app uses a single AMQP connection. Each handler is given its own dedicated channel associated with the single connection.
    /// The handlers then run in their own spawned tokio tasks.
    /// Handlers handle requests concurrently by spawning new tokio tasks for each incoming request.
    ///
    /// # Errors
    /// Returns an `Err` on any of the below conditions:
    /// * No handlers were registered.
    /// * A connection to the AMQP broker could not be established.
    /// * Queue/consumer declaration or binding failed while setting up a handler.
    ///
    /// # Panics
    /// On connection errors after the initial connection is established, the app will simply panic.
    pub async fn run(self, amqp_addr: &str) -> Result<()> {
        // Ensure some handlers were actually registered.
        if self.handlers.is_empty() {
            return Err(Error::NoHandlers);
        }

        debug!("Connecting to AMQP on address: {amqp_addr:?} ...");
        let conn = Connection::connect(amqp_addr, ConnectionProperties::default()).await?;
        trace!("Connected to AMQP on address: {amqp_addr:?}");

        // If we get an error from RabbitMQ, we simply panic. We leave it as an exercise to the reader to restart.
        // Could perhaps allow to customize this with a saved closure on the app struct.
        conn.on_error(|e| {
            panic!("Connection returned error: {e:#}");
        });

        let mut join_handles = Vec::new();

        let state = Arc::new(self.state);

        for task_factory in self.handlers.into_iter() {
            debug!(
                "Spawning handler task for routing key: {:?} ...",
                task_factory.routing_key()
            );

            // Construct the task from the factory. This produces a pinned future which we can then spawn.
            let task = task_factory.build(&conn, state.clone()).await?;

            // Spawn the task and save the join handle.
            join_handles.push(tokio::spawn(task));
        }

        info!(
            "Connected to AMQP broker. Listening on {} handler{}.",
            join_handles.len(),
            if join_handles.len() == 1 { "" } else { "s" }
        );

        // Await all the handlers tasks. Handlers receive messages in a loop so under normal conditions, this should never return.
        //
        // However, it can return in rare cases:
        // 1. A handler panicked.
        // 2. The AMQP broker cancelled a handler's consumer for some reason (maybe the AMQP broker itself crashed?).
        // 3. The AMQP connection somehow got an error (could be as innocent as a lost internet connection).
        //
        // In any case, the solution is the same. We simply shut down. We leave it to the user to restart.
        let (returning_handler, _remaining_handlers_count, _leftover_handlers) =
            select_all(join_handles).await;

        match returning_handler {
            Ok(routing_key) => {
                // This case can only happen if the handler task runs to completion.
                // I.e. it completes the loop of consuming messages. This should only happen if the consumer is cancelled somehow.
                panic!("A handler task for routing key {routing_key:?} returned unexpectedly! Was the consumer cancelled?");
            }
            Err(e) => {
                // The JoinError is either a task cancellation or a panic.
                // We don't cancel tasks so this must be a handler panic.
                panic!("A handler panicked: {:#}", e);
            }
        }
    }
}
