//! App IDs defined in the request.

use std::convert::Infallible;

use async_trait::async_trait;

use crate::{Extract, Request};

/// App ID extracted from the properties of the incoming request. Notice that this is already
/// logged as part of handling the request.
#[derive(Debug, Clone)]
pub struct AppId(pub Option<String>);

impl AppId {
    /// Returns the underlying `app_id` if it was present in the request. Alternatively the
    /// `default` is returned.
    pub fn unwrap_or<'a>(&'a self, default: &'a str) -> &'a str {
        self.0.as_deref().unwrap_or(default)
    }
}

#[async_trait]
impl Extract for AppId {
    type Error = Infallible;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        let app_id = req.app_id().map(|app_id| app_id.to_string());
        Ok(Self(app_id))
    }
}
