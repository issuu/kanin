//! Allows extracting app state.

use async_trait::async_trait;
use derive_more::{Deref, DerefMut};
use tracing::error;

use crate::{error::ServerError, Extract, HandlerError, Request};

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
impl<T: Clone + Send + Sync + 'static> Extract for State<T> {
    type Error = HandlerError;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        match req.state::<T>() {
            None => {
                error!("Attempted to retrieve state of type {}, but that type has not been added to the app state. Add it with `app.state(...)`", std::any::type_name::<T>());
                Err(HandlerError::Internal(ServerError::StateNotFound))
            }
            Some(t) => Ok(State(t.clone())),
        }
    }
}
