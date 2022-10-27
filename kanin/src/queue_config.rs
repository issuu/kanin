//! Holds [QueueConfig]'s implementation.

use lapin::options::QueueDeclareOptions;
use lapin::types::{AMQPValue, FieldTable};

/// Detailed configuration of queue.
pub struct QueueConfig {
    /// The exchange that the queue will be bound to.
    pub(crate) exchange: Exchange,
    /// Prefetch for queue.
    pub(crate) prefetch: u16,
    /// Queue declare options.
    pub(crate) options: QueueDeclareOptions,
    /// Queue arguments (aka. x-arguments).
    pub(crate) arguments: FieldTable,
}

impl QueueConfig {
    /// The default value for the prefetch count.
    pub const DEFAULT_PREFETCH: u16 = 64;

    /// Creates a new default QueueConfig.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the exchange of the handler.
    pub fn with_exchange(mut self, exchange: Exchange) -> Self {
        self.exchange = exchange;
        self
    }

    /// Per consumer prefetch count. See [documentation](https://www.rabbitmq.com/confirms.html#channel-qos-prefetch).
    pub fn with_prefetch(mut self, prefetch: u16) -> Self {
        self.prefetch = prefetch;
        self
    }

    /// Overwrite the `auto-delete` property for the queue (defaults to `true`).
    /// See [documentation](https://www.rabbitmq.com/queues.html#properties)
    pub fn with_auto_delete(mut self, auto_delete: bool) -> Self {
        self.options.auto_delete = auto_delete;
        self
    }

    /// Queues will expire after a period of time only when they are not used (e.g. do not have consumers).
    /// See [documentation](https://www.rabbitmq.com/ttl.html#queue-ttl).
    /// Value is in milliseconds, so `3600000 = 1 hour`.
    pub fn with_expires(mut self, expires: u32) -> Self {
        self.arguments
            .insert("x-expires".into(), AMQPValue::LongUInt(expires));
        self
    }

    /// Messages expires if not consumed within `message_ttl` milliseconds.
    /// See [documentation](https://www.rabbitmq.com/ttl.html#message-ttl-using-x-args).
    /// Value is in milliseconds, so `3600000 = 1 hour`.
    pub fn with_message_ttl(mut self, message_ttl: u32) -> Self {
        self.arguments
            .insert("x-message-ttl".into(), AMQPValue::LongUInt(message_ttl));
        self
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            exchange: Default::default(),
            prefetch: Self::DEFAULT_PREFETCH,
            options: QueueDeclareOptions {
                auto_delete: true,
                ..Default::default()
            },
            arguments: Default::default(),
        }
    }
}

/// AMQP exchanges. Currently only the default, direct and topic exchange are supported.
#[derive(Clone, Copy)]
pub enum Exchange {
    /// The default exchange is indicated by the empty string in AMQP.
    /// Note that the default exchange is actually just a direct exchange with no name.
    Default,
    /// The direct exchange. See https://www.rabbitmq.com/tutorials/tutorial-four-python.html for more information.
    Direct,
    /// The topic exchange. See https://www.rabbitmq.com/tutorials/tutorial-five-python.html for more information.
    Topic,
}

impl Exchange {
    /// The string representation of the exchange. This is what is sent to RabbitMQ to identify the exchange.
    pub(crate) fn name(&self) -> &str {
        match self {
            Exchange::Default => "",
            Exchange::Direct => "amq.direct",
            Exchange::Topic => "amq.topic",
        }
    }
}

impl Default for Exchange {
    fn default() -> Self {
        Exchange::Default
    }
}
