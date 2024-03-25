use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use lapin::{
    options::BasicPublishOptions,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel,
};
use tokio::sync::{mpsc::Sender, OnceCell};
use tracing::info;

use crate::{
    error::FromError,
    extract::{AppId, ReqId, State},
    tests::init_logging,
    App, Extract, HandlerError, Request, Respond,
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
            HandlerError::InvalidRequest(e) => MyResponse(format!("Invalid request: {e:#?}")),
        }
    }
}

#[async_trait]
impl<S> Extract<S> for MyResponse
where
    S: Send + Sync,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(MyResponse(
            String::from_utf8_lossy(&req.delivery().data).to_string(),
        ))
    }
}

static SYNC: OnceCell<Sender<()>> = OnceCell::const_new();

async fn handler() -> MyResponse {
    info!("handler");
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler".into())
}

async fn handler_message(request: MyResponse, state: State<Arc<Mutex<Vec<String>>>>) {
    info!("received message {request:?}");
    state.lock().unwrap().push("handler_message".into());
    SYNC.get().unwrap().send(()).await.unwrap();
}

async fn handler_channel(_channel: Channel) -> MyResponse {
    info!("handler_channel");
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_channel".into())
}

async fn handler_req_id(req_id: ReqId) -> MyResponse {
    info!("handler_req_id: {req_id}");
    assert_eq!(req_id.to_string(), "abc");
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_req_id".into())
}

async fn handler_app_id(AppId(app_id): AppId) -> MyResponse {
    info!("handler_app_id: {app_id:?}");
    assert_eq!(app_id.unwrap(), "my_app_id");
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_app_id".into())
}

async fn handler_two_extractors(_channel: Channel, app_id: AppId) -> MyResponse {
    info!("handler_two_extractors: {app_id:?}");
    SYNC.get().unwrap().send(()).await.unwrap();
    MyResponse("handler_two_extractors".into())
}

async fn handler_state_extractor(state: State<Arc<Mutex<Vec<String>>>>) -> MyResponse {
    info!("handler_state_extractor: {state:?}");
    state.lock().unwrap().push("handler_state_extractor".into());
    SYNC.get().unwrap().send(()).await.unwrap();

    MyResponse(format!("handler_state_extractor: {state:?}"))
}

/// A handler that doesn't respond just doesn't return anything.
async fn listener(state: State<Arc<Mutex<Vec<String>>>>) {
    info!("listener: {state:?}");
    state.lock().unwrap().push("listener".into());
    SYNC.get().unwrap().send(()).await.unwrap();
    // We just care about changing the state here, we don't want to reply with anything.
}

#[derive(Clone)]
struct SendState(Arc<Mutex<Vec<String>>>);

#[derive(Clone)]
struct RecvState(Arc<Mutex<Vec<String>>>);

impl From<&SendState> for Arc<Mutex<Vec<String>>> {
    fn from(state: &SendState) -> Self {
        state.0.clone()
    }
}

impl From<&RecvState> for Arc<Mutex<Vec<String>>> {
    fn from(state: &RecvState) -> Self {
        state.0.clone()
    }
}

#[tokio::test]
async fn it_receives_various_messages_and_works_as_expected() {
    init_logging();
    info!("Connecting to AMQP...");
    let conn = amqp_connect().await;

    // We use a shared state to recall the calls that happened.
    let send_state = SendState(Arc::new(Mutex::new(Vec::<String>::new())));
    let recv_state = RecvState(Arc::new(Mutex::new(Vec::<String>::new())));

    info!("Setting up send app...");
    let send_app = App::new(send_state.clone())
        .handler("handler", handler)
        .handler("handler_channel", handler_channel)
        .handler("handler_req_id", handler_req_id)
        .handler("handler_app_id", handler_app_id)
        .handler("handler_two_extractors", handler_two_extractors)
        .handler("handler_state_extractor", handler_state_extractor)
        .handler("listener", listener);

    let send_app_shutdown = send_app.shutdown_channel();
    let send_conn = amqp_connect().await;
    let send_app = send_app.run_with_connection(&send_conn);

    info!("Setting up recv app...");
    let recv_app = App::new(recv_state.clone())
        .handler("handler_reply_to", handler)
        .handler("handler_message_reply_to", handler_message);

    let recv_app_shutdown = recv_app.shutdown_channel();
    let recv_conn = amqp_connect().await;
    let recv_app = recv_app.run_with_connection(&recv_conn);

    let requests = async {
        tokio::time::sleep(Duration::from_secs(5)).await;

        let channel = conn
            .create_channel()
            .await
            .expect("failed to create channel");

        let send_msg = |routing_key: &'static str, reply_to: &'static str| async {
            let mut headers = FieldTable::default();
            headers.insert("req_id".into(), AMQPValue::LongString("abc".into()));

            info!("Basic publish...");
            channel
                .basic_publish(
                    "",
                    routing_key,
                    BasicPublishOptions::default(),
                    &[],
                    BasicProperties::default()
                        .with_reply_to(reply_to.into())
                        .with_app_id("my_app_id".into())
                        .with_headers(headers),
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
        info!("Sending messages...");
        let (send, mut recv) = tokio::sync::mpsc::channel(1);
        SYNC.set(send).unwrap();
        info!("Sending message handler_message...");
        send_msg("handler", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        info!("Sending message handler_channel...");
        send_msg("handler_channel", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        info!("Sending message handler_req_id...");
        send_msg("handler_req_id", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        info!("Sending message handler_app_id...");
        send_msg("handler_app_id", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        info!("Sending message handler_two_extractors...");
        send_msg("handler_two_extractors", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        info!("Sending message handler_state_extractor...");
        send_msg("handler_state_extractor", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();
        info!("Sending message listener...");
        send_msg("listener", "handler_message_reply_to").await;
        recv.recv().await.unwrap();
        recv.recv().await.unwrap();

        // Gracefully shutdown the apps at the end.
        info!("Sending shutdown signals...");
        send_app_shutdown.send(()).unwrap();
        recv_app_shutdown.send(()).unwrap();
    };

    // Verify that we shut down the apps.
    let (send_return, recv_return, ()) = tokio::join!(send_app, recv_app, requests);
    assert!(send_return.is_ok());
    assert!(recv_return.is_ok());

    // Unwrap the calls from the Arc.
    let send_calls = Arc::try_unwrap(send_state.0)
        .expect("Only one reference left (this one)")
        .into_inner()
        .expect("No one has a lock to the Mutex");
    let recv_calls = Arc::try_unwrap(recv_state.0)
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
            // "handler_req_id",
            // "handler_app_id",
            // "handler_two_extractors",
            "handler_state_extractor",
            "listener",
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
            "handler_message",
        ],
        recv_calls.as_ref()
    );
}
