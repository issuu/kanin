//! Interface for types that can extract themselves from requests.

mod app_id;
mod message;
mod req_id;
mod state;

use std::{convert::Infallible, error::Error};

use async_trait::async_trait;
use lapin::{acker::Acker, message::Delivery, Channel};

use crate::{error::HandlerError, Request};

pub use app_id::AppId;
pub use message::Msg;
pub use req_id::ReqId;
pub use state::State;

/// A trait for types that can be extracted from [requests](`Request`).
///
/// Note that extractions might mutate the request in certain ways.
/// Most notably, if extracting the [`Delivery`] or [`Acker`] from a request, it is the responsibility of the handler to acknowledge the message.
#[async_trait]
pub trait Extract: Sized {
    /// The error to return in case extraction fails.
    type Error: Error;

    /// Extract the type from the request.
    async fn extract(req: &mut Request) -> Result<Self, Self::Error>;
}

/// Note that when you extract the [`Delivery`], the handler itself must acknowledge the request.
/// Kanin *will not* acknowledge the request for you in this case.
#[async_trait]
impl Extract for Delivery {
    type Error = HandlerError;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        req.delivery
            .take()
            .ok_or(HandlerError::DELIVERY_ALREADY_EXTRACTED)
    }
}

/// Note that when you extract the [`Acker`], the handler itself must acknowledge the request.
/// kanin *will not* acknowledge the request for you in this case.
#[async_trait]
impl Extract for Acker {
    type Error = HandlerError;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        req.delivery
            .as_mut()
            .ok_or(HandlerError::DELIVERY_ALREADY_EXTRACTED)
            .map(|d| std::mem::take(&mut d.acker))
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
