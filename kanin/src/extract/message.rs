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
impl<S, D> Extract<S> for Msg<D>
where
    S: Send + Sync,
    D: Default + ProstMessage,
{
    type Error = HandlerError;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(Msg(D::decode(&req.delivery().data[..])?))
    }
}
