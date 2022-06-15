//! Interface for types that can extract themselves from requests.

mod message;
mod state;

use std::convert::Infallible;

use async_trait::async_trait;
use lapin::{message::Delivery, Channel};

use crate::{error::HandlerError, Request};

pub use message::Msg;
pub use state::State;

/// A trait for types that can be extracted from requests.
#[async_trait]
pub trait Extract: Sized {
    /// The error to return in case extraction fails.
    type Error;

    /// Extract the type from the request.
    async fn extract(req: &mut Request) -> Result<Self, Self::Error>;
}

#[async_trait]
impl Extract for Delivery {
    type Error = HandlerError;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        req.take_delivery()
            .ok_or(HandlerError::DELIVERY_ALREADY_EXTRACTED)
    }
}

#[async_trait]
impl Extract for Channel {
    type Error = Infallible;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        Ok(req.channel().clone())
    }
}

/// Extracting options simply discards the error and returns None in that case.
#[async_trait]
impl<T> Extract for Option<T>
where
    T: Extract,
{
    type Error = Infallible;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        Ok(Extract::extract(req).await.ok())
    }
}

#[async_trait]
impl<T> Extract for Result<T, <T as Extract>::Error>
where
    T: Extract,
{
    type Error = Infallible;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        Ok(Extract::extract(req).await)
    }
}
