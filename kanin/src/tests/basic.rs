use std::sync::{Arc, Mutex};

use lapin::Channel;

use crate::{
    error::FromError,
    extract::{AppId, State},
    App, AppState, HandlerError, Respond,
};

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
        }
    }
}

async fn handler() -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_channel(_channel: Channel) -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_two_extractors(_channel: Channel, _app_id: AppId) -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_state_extractor(state: State<Arc<Mutex<u32>>>) -> MyResponse {
    let mut request_count = state.lock().unwrap();
    *request_count += 1;

    MyResponse("hello".into())
}

/// A handler that doesn't respond just doesn't return anything.
async fn listener(state: State<Arc<Mutex<u32>>>) {
    let mut request_count = state.lock().unwrap();
    // We just care about changing the state here, we don't want to reply with anything.
    *request_count += 1;
}

#[derive(AppState)]
struct MyAppState(Arc<Mutex<u32>>);

// This also works.
#[allow(dead_code)]
#[derive(AppState)]
struct MyNamedAppState {
    my_state: Arc<Mutex<u32>>,
}

/// At the moment, this just verifies that the above handlers compile and work as handlers.
#[tokio::test]
async fn it_compiles() {
    let _ignore = App::new(MyAppState(Arc::new(Mutex::new(187))))
        .handler("routing_key_0", handler)
        .handler("routing_key_1", handler_with_channel)
        .handler("routing_key_3", handler_with_two_extractors)
        .handler("routing_key_4", handler_with_state_extractor)
        .handler("routing_key_5", listener);
}
