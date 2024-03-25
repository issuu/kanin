//! Module for the [App] struct and surrounding utilities.

mod task;

use std::sync::Arc;

use futures::{future::try_join_all, stream::FuturesUnordered, StreamExt};
use lapin::{self, Connection, ConnectionProperties};
use metrics::describe_gauge;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
use tokio::{sync::broadcast, task::JoinHandle};
use tracing::{debug, error, info, trace, warn};

use self::task::TaskFactory;
use crate::{Error, Handler, HandlerConfig, Respond, Result};

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
    /// Shutdown channel. Used to indicate that we should start graceful shutdown.
    /// The channel has capacity 1 as we only need to signal once to shutdown.
    /// Missing messages on the channel doesn't matter.
    shutdown: broadcast::Sender<()>,
}

impl<S: Default> Default for App<S> {
    fn default() -> Self {
        Self {
            handlers: Vec::default(),
            state: S::default(),
            shutdown: broadcast::Sender::new(1),
        }
    }
}

impl<S> App<S> {
    /// Creates a new kanin app.
    pub fn new(state: S) -> Self {
        Self {
            handlers: Vec::new(),
            state,
            shutdown: broadcast::Sender::new(1),
        }
    }

    /// Returns a [`broadcast::Sender<()>`]. If you send a message on this channel, the app will gracefully shut down.
    pub fn shutdown_channel(&self) -> broadcast::Sender<()> {
        self.shutdown.clone()
    }

    /// Sets up signal handling to gracefully shut down the app when
    /// this process receives termination signals from the operating system.
    ///
    /// This is a convenience function. If you want custom shutdown behavior, you can
    /// use the broadcast channel returned from the [`Self::shutdown_channel`] method.
    ///
    /// This functions sets up listeners for shutdown events. For non-Unix platforms, it uses [`tokio::signal::ctrl_c`].
    /// For Unix platforms, it sets up listeners for SIGTERM, SIGINT and SIGHUP.
    ///
    /// # Panics
    /// The background listening task spawned by this function will panic on Unix if it fails to setup any of the signal listeners.
    /// In this case, signals will not be listened to and graceful shutdown will not start if signals are sent to the process.
    pub fn graceful_shutdown_on_signal(self) -> Self {
        let shutdown = self.shutdown_channel();
        tokio::spawn(async move {
            #[cfg(not(unix))]
            {
                // This should cover ctrl-c in most platforms.
                let signal = tokio::signal::ctrl_c().await;

                if let Err(e) = signal {
                    error!("Failed to listen for ctrl-c: {e}")
                }

                info!("Received ctrl-c. Attempting to gracefully shut down...");
            }

            // We'll be more specific for Unix signal handling.
            #[cfg(unix)]
            {
                // SIGTERM is commonly sent for graceful shutdown of applications, followed by 30 seconds of grace time, then a SIGKILL.
                let mut sigterm =
                    signal(SignalKind::terminate()).expect("failed to listen for SIGTERM");
                // SIGINT is usually sent due to ctrl-c in the terminal.
                let mut sigint =
                    signal(SignalKind::interrupt()).expect("failed to listen for SIGINT");
                // SIGHUP is usually sent when the terminal closes or the user logs out (for instance logs out of an SSH session).
                let mut sighup = signal(SignalKind::hangup()).expect("failed to listen for SIGHUP");

                tokio::select! {
                    _ = sigterm.recv() => info!("Received SIGTERM. Attempting to gracefully shut down..."),
                    _ = sigint.recv() => info!("Received SIGINT. Attempting to gracefully shut down..."),
                    _ = sighup.recv() => info!("Received SIGHUP. Attempting to gracefully shut down..."),
                };
            }

            if let Err(e) = shutdown.send(()) {
                error!("Failed to send shutdown message: {e}")
            }
        });

        self
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
        // Describe metrics (just need to do it somewhere once as we run the app).
        describe_gauge!("kanin.prefetch_capacity", "A gauge that measures how much prefetch is available on a certain queue, based on the prefetch of its consumers.");

        let shutdown_channel = self.shutdown_channel();
        let mut handles = self.setup_handlers(conn).await?;

        let mut ret = Ok(());
        while let Some(returning_handler) = handles.next().await {
            match returning_handler {
                Ok(Ok(())) => {
                    // Graceful handler shutdown, do nothing.
                    // If all goes well, all handlers will go into this branch
                    // and eventually we'll be done.
                }
                Ok(Err(e)) => {
                    // Consumer cancellation from AMQP broker.
                    if let Err(e) = shutdown_channel.send(()) {
                        error!("Failed to send shutdown signal to other tasks on consumer cancellation: {e}");
                    }
                    ret = Err(e);
                }
                Err(e) => {
                    // Panic from kanin's own internal task handling.
                    // This is not a panic in the downstream user-created handlers,
                    // those don't cause an exit from the app.
                    panic!("A kanin task panicked: {e:#}");
                }
            }
        }

        info!("Gracefully shutdown. Goodbye.");

        ret
    }

    /// Set up all the handlers, returning a collection of all the join handles.
    pub(crate) async fn setup_handlers(
        self,
        conn: &Connection,
    ) -> Result<FuturesUnordered<JoinHandle<Result<()>>>> {
        if self.handlers.is_empty() {
            return Err(Error::NoHandlers);
        }

        let conn_err_shutdown = self.shutdown.clone();
        // If the connection fails, we try to signal for a graceful shutdown.
        conn.on_error(move |e| {
            error!("Connection returned error: {e:#}");
            if let Err(e) = conn_err_shutdown.send(()) {
                warn!("Could not send shutdown signal; are all handlers shut down already? Error: {e:#}");
            }
        });

        let state = Arc::new(self.state);
        let join_handles = try_join_all(self.handlers.into_iter().map(|task_factory| async {
            debug!(
                "Spawning handler task for routing key: {:?} ...",
                task_factory.routing_key()
            );

            // Construct the task from the factory. This produces a pinned future which we can then spawn.
            let task = task_factory
                .build(conn, state.clone(), self.shutdown.subscribe())
                .await
                .map_err(Error::Lapin)?;

            // Spawn the task and save the join handle.
            Ok(tokio::spawn(task))
        }))
        .await?;

        info!(
            "Connected to AMQP broker. Listening on {} handler{}.",
            join_handles.len(),
            if join_handles.len() == 1 { "" } else { "s" }
        );

        Ok(join_handles.into_iter().collect())
    }
}
