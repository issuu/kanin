//! Request IDs.

use core::fmt;
use std::convert::Infallible;

use async_trait::async_trait;
use lapin::types::{AMQPValue, LongString};
use uuid::Uuid;

use crate::{Extract, Request};

/// Request id.
pub struct ReqId(AMQPValue);

impl ReqId {
    /// Create a new ReqId with a random uuid.
    fn new() -> Self {
        let uuid = Uuid::new_v4();
        let amqp_value = AMQPValue::LongString(LongString::from(uuid.to_string()));
        Self(amqp_value)
    }
}

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
