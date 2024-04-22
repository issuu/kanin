//! Interface for types that can extract themselves from requests.

mod acker;
mod app_id;
mod message;
mod req_id;
mod state;

pub use acker::Acker;
pub use app_id::AppId;
pub use message::Msg;
pub use req_id::ReqId;
pub use state::State;

use std::{convert::Infallible, error::Error};

use async_trait::async_trait;
use lapin::Channel;

use crate::Request;

/// A trait for types that can be extracted from [requests](`Request`).
///
/// Note that extractions might mutate the request in certain ways.
#[async_trait]
pub trait Extract<S>: Sized {
    /// The error to return in case extraction fails.
    type Error: Error;

    /// Extract the type from the request.
    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error>;
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

/// Extracting a result returns the extraction error if it fails, allowing the handler to decide what to do with the error.
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
