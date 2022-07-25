use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::future::join_all;
use lapin::{message::Delivery, options::BasicPublishOptions, BasicProperties, Channel};
use log::info;
use tokio::sync::{mpsc::Sender, OnceCell};

use crate::{
    error::FromError, extract::State, tests::init_logging, App, Extract, HandlerError, Request,
    Respond,
};

use super::amqp_connect;

#[derive(Debug)]
struct MyResponse(String);

impl Respond for MyResponse {
    fn respond(self) -> Vec<u8> {
        self.0.into()
    }
}

impl FromError<HandlerError> for MyResponse {
    fn from_error(error: HandlerError) -> Self {
        match error {
            HandlerError::InvalidRequest(e) => MyResponse(format!("Invalid request: {:#?}", e)),
            HandlerError::Internal(e) => MyResponse(format!("Internal server error: {:#?}", e)),
        }
    }
}

#[async_trait]
impl Extract for MyResponse {
    type Error = HandlerError;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        match &req.delivery {
            None => Err(HandlerError::DELIVERY_ALREADY_EXTRACTED),
            Some(d) => Ok(MyResponse(String::from_utf8_lossy(&d.data).to_string())),
        }
    }
}

static SYNC: OnceCell<Sender<()>> = OnceCell::const_new();

async fn handler() -> MyResponse {
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler".into())
}

async fn handler_message(request: MyResponse, state: State<Arc<Mutex<Vec<String>>>>) {
    state.lock().unwrap().push("handler_message".into());
    SYNC.get().unwrap().send(()).await.unwrap();

    info!("received message {request:?}")
}

async fn handler_channel(_channel: Channel) -> MyResponse {
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_channel".into())
}

async fn handler_delivery(_delivery: Delivery) -> MyResponse {
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_delivery".into())
}

async fn handler_two_extractors(_channel: Channel, _delivery: Delivery) -> MyResponse {
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_two_extractors".into())
}

async fn handler_state_extractor(state: State<Arc<Mutex<Vec<String>>>>) -> MyResponse {
    state.lock().unwrap().push("handler_state_extractor".into());
    SYNC.get().unwrap().send(()).await.unwrap();

    MyResponse(format!("handler_state_extractor: {:?}", state))
}

/// A handler that doesn't respond just doesn't return anything.
async fn listener(state: State<Arc<Mutex<Vec<String>>>>) {
    state.lock().unwrap().push("listener".into());
    SYNC.get().unwrap().send(()).await.unwrap();
    // We just care about changing the state here, we don't want to reply with anything.
}

/// Shuts down the app by panicking.
async fn shutdown(state: State<Arc<Mutex<Vec<String>>>>) {
    state.lock().unwrap().push("shutdown".into());

    panic!("Shutdown");
}

/// At the moment, this just verifies that the above handlers compile and work as handlers.
#[tokio::test]
async fn it_receives_various_messages_and_works_as_expected() {
    init_logging();
    let conn = amqp_connect().await;

    // We use a shared state to recall the calls that happened.
    let send_state = Arc::new(Mutex::new(Vec::<String>::new()));
    let recv_state = Arc::new(Mutex::new(Vec::<String>::new()));

    let send_app = App::new()
        .state(send_state.clone())
        .handler("handler", handler)
        .handler("handler_channel", handler_channel)
        .handler("handler_delivery", handler_delivery)
        .handler("handler_two_extractors", handler_two_extractors)
        .handler("handler_state_extractor", handler_state_extractor)
        .handler("listener", listener)
        .handler("shutdown", shutdown)
        .setup_handlers(&amqp_connect().await)
        .await
        .unwrap();

    let recv_app = App::new()
        .state(recv_state.clone())
        .handler("handler_reply_to", handler)
        .handler("handler_message_reply_to", handler_message)
        .handler("recv_shutdown", shutdown)
        .setup_handlers(&amqp_connect().await)
        .await
        .unwrap();

    let requests = async {
        let channel = conn
            .create_channel()
            .await
            .expect("failed to create channel");

        let send_msg = |routing_key: &'static str, reply_to: &'static str| async {
            channel
                .basic_publish(
                    "",
                    routing_key,
                    BasicPublishOptions::default(),
                    &[],
                    BasicProperties::default().with_reply_to(reply_to.into()),
                )
                .await
                .expect("failed to publish");
        };

        // Okay so here's where it goes down. We send messages to the sender app, which will reply on the given reply_to property.
        // The replies will be picked up by the receive_app. This is to ensure that kanin can successfully actually send messages.
        // We have a state in both of the apps that we use to store what calls happened. So now we send lots of messages and then
        // we can verify afterwards that the messages actually resulted in the right calls.
        // We wait in between calls for the handlers to reply to a static channel.
        // This is just to ensure that we only send the next message when we know that the apps have alredy handled the current one.
        // We wait for two messages, one for each app.
        let (send, mut recv) = tokio::sync::mpsc::channel(1);
        SYNC.set(send).unwrap();
        send_msg("handler", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        send_msg("handler_channel", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        send_msg("handler_delivery", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        send_msg("handler_two_extractors", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        send_msg("handler_state_extractor", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        send_msg("listener", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();

        // Shutdown the apps by making them panic.
        send_msg("shutdown", "shutdown_reply_to").await;
        send_msg("recv_shutdown", "shutdown_reply_to").await;
    };

    // Verify that we shut down the apps.
    info!("join");
    let ((send_return, _, send_tasks), (recv_return, _, recv_tasks), ()) =
        tokio::join!(send_app, recv_app, requests);
    assert!(send_return.unwrap_err().is_panic());
    assert!(recv_return.unwrap_err().is_panic());

    // Shutdown all the remaining handlers.
    // This is required for the try_unwrap calls below,
    // because the tasks here still hold refernces to the Arc.
    for task in &send_tasks {
        task.abort();
    }
    for task in &recv_tasks {
        task.abort();
    }
    join_all(send_tasks).await;
    join_all(recv_tasks).await;

    // Unwrap the calls from the Arc.
    let send_calls = Arc::try_unwrap(send_state)
        .expect("Only one reference left (this one)")
        .into_inner()
        .expect("No one has a lock to the Mutex");
    let recv_calls = Arc::try_unwrap(recv_state)
        .expect("Only one reference left (this one)")
        .into_inner()
        .expect("No one has a lock to the Mutex");

    // Now to verify the call order. From the above messages, the order is deterministic.
    assert_eq!(
        [
            // These are called but don't change the state.
            // We verify that they are called by seeing that they call the recv_app.
            // "handler",
            // "handler_channel",
            // "handler_delivery",
            // "handler_two_extractors",
            "handler_state_extractor",
            "listener",
            "shutdown",
        ],
        send_calls.as_ref()
    );

    assert_eq!(
        [
            "handler_message",
            "handler_message",
            "handler_message",
            "handler_message",
            "handler_message",
            "handler_message",
            "shutdown",
        ],
        recv_calls.as_ref()
    );
}
