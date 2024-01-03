//! Module for the [App] struct and surrounding utilities.

mod task;

use std::sync::Arc;

use futures::future::{select_all, SelectAll};
use lapin::{self, Connection, ConnectionProperties};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace};

use self::task::TaskFactory;
use crate::{Error, Handler, HandlerConfig, Respond, Result};
use tokio::sync::mpsc;

/// The central struct of your application.
#[must_use = "The app will not do anything unless you call `.run`."]
pub struct App<S> {
    /// A map from routing keys to task factories.
    /// Task factories are constructed in [`App::handler`] and called in [`App::run`].
    handlers: Vec<TaskFactory<S>>,
    /// This is used to hold the state values that users may want to store before running the app,
    /// and then extract in their handlers. Types that wish to be extracted via `State<T>` must
    /// implement `From<&S>`.
    state: S,
}

impl<S: Default> Default for App<S> {
    fn default() -> Self {
        Self {
            handlers: Vec::default(),
            state: S::default(),
        }
    }
}

impl<S> App<S> {
    /// Creates a new kanin app with the default configuration.
    pub fn new(state: S) -> Self {
        Self {
            handlers: Vec::new(),
            state,
        }
    }

    /// Registers a new handler for the given routing key with the default prefetch count.
    ///
    /// The handler will respond to any messages with `reply_to` and `correlation_id` properties.
    /// This requires that the response type implements Respond (which is automatically implemented for protobuf messages).
    pub fn handler<H, Args, Res>(self, routing_key: impl Into<String>, handler: H) -> Self
    where
        H: Handler<Args, Res, S>,
        Res: Respond,
        S: Send + Sync + 'static,
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
        config: HandlerConfig,
    ) -> Self
    where
        H: Handler<Args, Res, S>,
        Res: Respond,
        S: Send + Sync + 'static,
    {
        let routing_key = routing_key.into();
        debug!(
            "Registering handler {} on routing key {routing_key:?} with config {config:?}",
            std::any::type_name::<H>()
        );

        // Create and save the task factory - this is a function that creates the async task that will be run in tokio.
        self.handlers
            .push(TaskFactory::new(routing_key, handler, config));

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
    // pub fn state<T: Clone + Send + Sync + 'static>(mut self, value: T) -> Self {
    //     debug!("Registering state for type {}", std::any::type_name::<T>());
    //     if self.state.insert(value).is_some() {
    //         panic!(
    //             "Attempted to register a state type, `{}` that had already been registered before! \
    //             You can only register one value of each type. If you need multiple values of the same type, \
    //             use the newtype pattern to signify the semantic difference between the two values.",
    //             std::any::type_name::<T>()
    //         );
    //     }
    //     self
    // }

    /// Connects to AMQP with the given address and calls [`run_with_connection`][App::run_with_connection] with the resulting connection.
    /// See [`run_with_connection`][App::run_with_connection] for more details.
    #[allow(clippy::missing_errors_doc)]
    #[inline]
    pub async fn run(self, amqp_addr: &str) -> Result<()> {
        debug!("Connecting to AMQP on address: {amqp_addr:?} ...");
        let conn = Connection::connect(amqp_addr, ConnectionProperties::default())
            .await
            .map_err(Error::Lapin)?;
        trace!("Connected to AMQP on address: {amqp_addr:?}");
        self.run_with_connection(&conn).await
    }

    /// Runs the app with all the handlers that have been registered.
    ///
    /// Each handler is given its own dedicated channel associated with the given connection.
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
    /// On connection errors, the app will simply panic.
    #[inline]
    pub async fn run_with_connection(self, conn: &Connection) -> Result<()> {
        let handles = self.setup_handlers(conn).await?;
        let (returning_handler, _remaining_handlers_count, _leftover_handlers) = handles.await;

        match returning_handler {
            Ok(routing_key) => {
                // This case can only happen if the handler task runs to completion.
                // I.e. it completes the loop of consuming messages. This should only happen if the consumer is cancelled somehow.
                panic!("A handler task for routing key {routing_key:?} returned unexpectedly! Was the consumer cancelled?");
            }
            Err(e) => {
                // The JoinError is either a task cancellation or a panic.
                // We don't cancel tasks so this must be a handler panic.
                panic!("A handler panicked: {e:#}");
            }
        }
    }

    /// Set up all the handlers, returning a [`SelectAll`] future that collects all the join handles.
    pub(crate) async fn setup_handlers(
        self,
        conn: &Connection,
    ) -> Result<SelectAll<JoinHandle<String>>> {
        if self.handlers.is_empty() {
            return Err(Error::NoHandlers);
        }

        // If the connection fails, we want to panic the entire application.
        // We could consider possibly trying a restart somehow, but that seems complicated. Simpler to just restart.
        let (send, mut recv) = mpsc::channel(1);
        conn.on_error(move |e| {
            error!("Connection returned error: {e:#}");
            send.blocking_send(())
                .expect("failed to send connection error message");
            panic!("panicking due to connection error");
        });

        let mut join_handles = Vec::new();
        let state = Arc::new(self.state);
        for task_factory in self.handlers.into_iter() {
            debug!(
                "Spawning handler task for routing key: {:?} ...",
                task_factory.routing_key()
            );

            // Construct the task from the factory. This produces a pinned future which we can then spawn.
            let task = task_factory
                .build(conn, state.clone())
                .await
                .map_err(Error::Lapin)?;

            // Spawn the task and save the join handle.
            join_handles.push(tokio::spawn(task));
        }
        info!(
            "Connected to AMQP broker. Listening on {} handler{}.",
            join_handles.len(),
            if join_handles.len() == 1 { "" } else { "s" }
        );

        // We add one additional task which merely listens for a message from the `on_error` closure on the connection.
        // This should ensure that we notify the user of the error via a panic if the connection runs into an error.
        join_handles.push(tokio::spawn(async move {
            recv.recv()
                .await
                .expect("failed to receive connection error message");
            panic!("received message indicating a connection error has occurred");
        }));

        Ok(select_all(join_handles))
    }
}
