//! App IDs defined in the request.

use std::convert::Infallible;

use async_trait::async_trait;

use crate::{Extract, Request};

/// App ID extracted from the properties of the incoming request. Notice that this is already
/// logged as part of handling the request.
#[derive(Debug, Clone)]
pub struct AppId(pub Option<String>);

#[async_trait]
impl<S> Extract<S> for AppId
where
    S: Send + Sync,
{
    type Error = Infallible;

    async fn extract(req: &mut Request<S>) -> Result<Self, Self::Error> {
        let app_id = req.app_id().map(|app_id| app_id.to_string());
        Ok(Self(app_id))
    }
}
