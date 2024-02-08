//! Interface for types that can extract themselves from requests.

mod app_id;
mod message;
mod req_id;
mod state;

pub use app_id::AppId;
pub use message::Msg;
pub use req_id::ReqId;
pub use state::State;

use std::{convert::Infallible, error::Error};

use async_trait::async_trait;
use lapin::{acker::Acker, Channel};

use crate::Request;

/// A trait for types that can be extracted from [requests](`Request`).
///
/// Note that extractions might mutate the request in certain ways.
/// Most notably, if extracting the [`Delivery`] or [`Acker`] from a request, it is the responsibility of the handler to acknowledge the message.
#[async_trait]
pub trait Extract<S>: Sized {
    /// The error to return in case extraction fails.
    type Error: Error;

    /// Extract the type from the request.
    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error>;
}

/// Note that when you extract the [`Acker`], the handler itself must acknowledge the request.
/// kanin *will not* acknowledge the request for you in this case.
// TODO: This implementation is quite hacky and should probably be removed.
#[async_trait]
impl<S> Extract<S> for Acker
where
    S: Send + Sync,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(std::mem::take(&mut req.delivery.acker))
    }
}

#[async_trait]
impl<S> Extract<S> for Channel
where
    S: Send + Sync,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(req.channel().clone())
    }
}

/// Extracting options simply discards the error and returns None in that case.
#[async_trait]
impl<S, T> Extract<S> for Option<T>
where
    T: Extract<S>,
    S: Send + Sync,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(Extract::extract(req).await.ok())
    }
}

#[async_trait]
impl<S, T> Extract<S> for Result<T, <T as Extract<S>>::Error>
where
    T: Extract<S>,
    S: Send + Sync,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(Extract::extract(req).await)
    }
}
