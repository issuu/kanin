//! Request IDs.

use core::fmt;
use std::convert::Infallible;

use async_trait::async_trait;
use lapin::types::{AMQPValue, LongString};
use uuid::Uuid;

use crate::{Extract, Request};

/// Request IDs allow concurrent logs to be associated with a unique request. It can also enable requests
/// to be traced between different services by propagating the request IDs when calling other services.
/// This type implements [`Extract`], so it can be used in handlers.
pub struct ReqId(pub AMQPValue);

impl ReqId {
    /// Create a new [`ReqId`] as a random UUID.
    fn new() -> Self {
        let uuid = Uuid::new_v4();
        let amqp_value = AMQPValue::LongString(LongString::from(uuid.to_string()));
        Self(amqp_value)
    }
}

/// [`AMQPValue`] does not implement `Display` but we provide a `Display` implementation for `ReqId` to allow it to be used in tracing spans (see the `tracing` crate).
impl fmt::Display for ReqId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            AMQPValue::LongString(req_id) => req_id.fmt(f),
            AMQPValue::Boolean(b) => b.fmt(f),
            AMQPValue::ShortShortInt(v) => v.fmt(f),
            AMQPValue::ShortShortUInt(v) => v.fmt(f),
            AMQPValue::ShortInt(v) => v.fmt(f),
            AMQPValue::ShortUInt(v) => v.fmt(f),
            AMQPValue::LongInt(v) => v.fmt(f),
            AMQPValue::LongUInt(v) => v.fmt(f),
            AMQPValue::LongLongInt(v) => v.fmt(f),
            AMQPValue::Float(v) => v.fmt(f),
            AMQPValue::Double(v) => v.fmt(f),
            AMQPValue::DecimalValue(v) => write!(f, "{v:?}"),
            AMQPValue::ShortString(v) => write!(f, "{v:?}"),
            AMQPValue::FieldArray(v) => write!(f, "{v:?}"),
            AMQPValue::Timestamp(v) => write!(f, "{v:?}"),
            AMQPValue::FieldTable(v) => write!(f, "{v:?}"),
            AMQPValue::ByteArray(v) => write!(f, "{v:?}"),
            AMQPValue::Void => write!(f, "Void"),
        }
    }
}

/// Extract implementation for protobuf messages.
#[async_trait]
impl Extract for ReqId {
    type Error = Infallible;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        let Some(delivery) = &req.delivery else {
            return Ok(Self::new());
        };

        let Some(headers) = delivery.properties.headers() else {
            return Ok(Self::new());
        };

        match headers.inner().get("req_id") {
            None => Ok(Self::new()),
            Some(req_id) => Ok(Self(req_id.clone())),
        }
    }
}
