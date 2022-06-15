use std::sync::Mutex;

use lapin::{message::Delivery, Channel};

use crate::{error::FromError, extract::State};

use super::*;

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

async fn handler() -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_channel(_channel: Channel) -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_delivery(_delivery: Delivery) -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_two_extractors(_channel: Channel, _delivery: Delivery) -> MyResponse {
    MyResponse("hello".into())
}

async fn handler_with_state_extractor(state: State<Mutex<u32>>) -> MyResponse {
    let mut request_count = state.lock().unwrap();
    *request_count += 1;

    MyResponse("hello".into())
}

/// A handler that doesn't respond just doesn't return anything.
async fn listener(state: State<Mutex<u32>>) {
    let mut request_count = state.lock().unwrap();
    // We just care about changing the state here, we don't want to reply with anything.
    *request_count += 1;
}

/// At the moment, this just verifies that the above handlers compile and work as handlers.
#[tokio::test]
async fn it_compiles() {
    let _ignore = App::new()
        .handler("routing_key_0", handler)
        .handler("routing_key_1", handler_with_channel)
        .handler("routing_key_2", handler_with_delivery)
        .handler("routing_key_3", handler_with_two_extractors)
        .handler("routing_key_4", handler_with_state_extractor)
        .handler("routing_key_5", listener)
        .run("amqp_address")
        .await;
}
