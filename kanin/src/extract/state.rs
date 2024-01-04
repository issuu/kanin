//! Allows extracting app state.

use std::convert::Infallible;

use async_trait::async_trait;
use derive_more::{Deref, DerefMut};

use crate::{Extract, Request};

/// `State` is an extractor helper struct that allows you to extract app state from the state type added in `App::new`.
///
/// This implements `Deref` and `DerefMut` to the inner type, but the inner type is also public so you can also just take ownership if you prefer.
///
/// Any type that implements `From<&S>` where `S` is the app state given in `App::new` can be extracted via this type.
/// These `From` implementations can be derived on a struct via `kanin::AppState`.
///
/// # Example
/// ```
/// # use kanin::{AppState, extract::State};
/// #[derive(AppState)]
/// struct AppState {
///     num: u8,
/// }
/// async fn my_handler(State(num): State<u8>) {
///     assert_eq!(42, num);
/// }
/// ```
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
