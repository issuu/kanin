//! [Handler]s are functions whose arguments can be constructed from the app or the incoming AMQP message.

use std::clone::Clone;
use std::future::Future;

use async_trait::async_trait;

use crate::{error::FromError, extract::Extract, request::Request, response::Respond};

/// A trait for functions that can be used as handlers for incoming AMPQ messages.
///
/// The trait implementations on functions of different arities allow handlers to have (almost) any number of parameters.
#[async_trait]
pub trait Handler<Args, Res: Respond>: Send + 'static + Clone {
    /// Calls the handler with the given request.
    async fn call(self, req: &mut Request) -> Res;
}

/// Special-case the 0-args case to avoid unused variable warnings.
#[async_trait]
impl<Func, Fut, Res> Handler<(), Res> for Func
where
    Func: FnOnce() -> Fut + Send + 'static + Clone,
    Fut: Future<Output = Res> + Send,
    Res: Respond,
{
    async fn call(self, _req: &mut Request) -> Res {
        self().await
    }
}

/// Implements the handler trait for any number of parameters for handlers that return a value.
macro_rules! impl_handler {
    ( $($ty:ident),* $(,)? ) => {
        #[allow(non_snake_case)]
        #[async_trait]
        impl<Func, Fut, Res, $($ty,)*> Handler<($($ty,)*), Res> for Func
        where
            Func: FnOnce($($ty,)*) -> Fut + Send + 'static + Clone,
            Fut: Future<Output = Res> + Send,
            Res: Respond,
            $( $ty: Extract + Send,)*
            $( Res: FromError<<$ty as Extract>::Error>,)*
        {
            async fn call(self, req: &mut Request) -> Res {
                $(
                    let $ty = match $ty::extract(req).await {
                        Ok(value) => value,
                        Err(error) => {
                            tracing::error!("{error}");
                            return Res::from_error(error);
                        }
                    };
                )*

                self($($ty,)*).await
            }
        }
    };
}

// Implement for up to 12 parameters.
impl_handler!(T1);
impl_handler!(T1, T2);
impl_handler!(T1, T2, T3);
impl_handler!(T1, T2, T3, T4);
impl_handler!(T1, T2, T3, T4, T5);
impl_handler!(T1, T2, T3, T4, T5, T6);
impl_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
