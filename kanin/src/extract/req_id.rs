//! Request IDs.

use core::fmt;
use std::convert::Infallible;

use async_trait::async_trait;
use lapin::{
    message::Delivery,
    types::{AMQPValue, LongString},
};
use uuid::Uuid;

use crate::{Extract, Request};

/// Request IDs allow concurrent logs to be associated with a unique request. It can also enable requests
/// to be traced between different services by propagating the request IDs when calling other services.
/// This type implements [`Extract`], so it can be used in handlers.
#[derive(Debug, Clone)]
pub struct ReqId(AMQPValue);

impl ReqId {
    /// Create a new [`ReqId`] as a random UUID.
    fn new() -> Self {
        let uuid = Uuid::new_v4();
        let amqp_value = AMQPValue::LongString(LongString::from(uuid.to_string()));
        Self(amqp_value)
    }
}

/// [`AMQPValue`] does not implement `Display` but we provide a `Display` implementation for
/// `ReqId` to allow it to be used in tracing spans (see the `tracing` crate).
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

impl From<&Delivery> for ReqId {
    fn from(delivery: &Delivery) -> Self {
        let Some(headers) = delivery.properties.headers() else {
            return Self::new();
        };

        let Some(req_id) = headers.inner().get("req_id") else {
            return Self::new();
        };

        Self(req_id.clone())
    }
}

#[async_trait]
impl Extract for ReqId {
    type Error = Infallible;

    async fn extract(req: &mut Request) -> Result<Self, Self::Error> {
        Ok(req.req_id.clone())
    }
}
