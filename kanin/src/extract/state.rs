//! Allows extracting app state.

use std::convert::Infallible;

use async_trait::async_trait;
use derive_more::{Deref, DerefMut};

use crate::{Extract, Request};

/// `State` is an extractor helper struct that allows you to extract app state
/// that has previously been added to the app through a call to [`crate::App::state`]
///
/// This implements `Deref` and `DerefMut` to the inner type.
#[derive(Debug, Deref, DerefMut)]
pub struct State<T>(pub T);

impl<T: Clone> Clone for State<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Extract implementation for app state.
#[async_trait]
impl<S, T> Extract<S> for State<T>
where
    S: Send + Sync,
    T: for<'a> From<&'a S>,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        Ok(Self(req.state::<T>()))
    }
}
