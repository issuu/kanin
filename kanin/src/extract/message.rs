//! Allows extracting protobuf messages.

use async_trait::async_trait;
use derive_more::{Deref, DerefMut};
use prost::Message as ProstMessage;

use crate::{error::HandlerError, Extract, Request};

/// A simple wrapper that allows you to extract a protobuf message.
#[derive(Debug, Deref, DerefMut)]
pub struct Msg<T>(pub T);

/// Extract implementation for protobuf messages.
#[async_trait]
impl<D> Extract for Msg<D>
where
    D: Default + ProstMessage,
{
    type Error = HandlerError;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        match req.delivery() {
            None => Err(HandlerError::DELIVERY_ALREADY_EXTRACTED),
            Some(d) => {
                let data: &[u8] = &d.data;
                Ok(Msg(D::decode(data)?))
            }
        }
    }
}
