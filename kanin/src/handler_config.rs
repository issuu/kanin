//! Holds [QueueConfig]'s implementation.

use std::time::Duration;

use lapin::options::QueueDeclareOptions;
use lapin::types::{AMQPValue, FieldTable, ShortString};

/// Detailed configuration of a handler.
#[derive(Clone, Debug)]
pub struct HandlerConfig {
    /// Queue name to bind to. By default, this will be the same as whatever routing key is used for the handler.
    pub(crate) queue: Option<String>,
    /// The exchange that the queue will be bound to.
    pub(crate) exchange: String,
    /// Prefetch for the queue.
    pub(crate) prefetch: u16,
    /// Queue declare options.
    pub(crate) options: QueueDeclareOptions,
    /// Queue arguments (aka. x-arguments).
    pub(crate) arguments: FieldTable,
    /// True indicates that the handler should reply to messages (the default).
    /// False indicates that the handler should *not* reply to messages.
    ///
    /// Note that using `()` as the response type from a handler is not sufficient for making the handler not respond,
    /// as `()` implements [`prost::Message`], making it a valid protobuf response message.
    pub(crate) should_reply: bool,
}

impl HandlerConfig {
    /// The default value for the prefetch count.
    pub const DEFAULT_PREFETCH: u16 = 64;

    /// The default exchange is indicated by the empty string in AMQP.
    /// Note that the default exchange is actually just a direct exchange with no name.
    pub const DEFAULT_EXCHANGE: &str = "";

    /// The direct exchange. See <`https://www.rabbitmq.com/tutorials/tutorial-four-python.html`> for more information.
    pub const DIRECT_EXCHANGE: &str = "amq.direct";

    /// The topic exchange. See <`https://www.rabbitmq.com/tutorials/tutorial-five-python.html`> for more information.
    pub const TOPIC_EXCHANGE: &str = "amq.topic";

    /// Creates a new default QueueConfig.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the exchange of the handler. Defaults to the direct exchange, [`QueueConfig::DIRECT_EXCHANGE`].
    pub fn with_queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = Some(queue.into());
        self
    }

    /// Sets the exchange of the handler. Defaults to the direct exchange, [`QueueConfig::DIRECT_EXCHANGE`].
    pub fn with_exchange(mut self, exchange: impl Into<String>) -> Self {
        self.exchange = exchange.into();
        self
    }

    /// Per consumer prefetch count. See [documentation](https://www.rabbitmq.com/confirms.html#channel-qos-prefetch).
    pub fn with_prefetch(mut self, prefetch: u16) -> Self {
        self.prefetch = prefetch;
        self
    }

    /// Overwrite the `auto-delete` property for the queue (defaults to `true`).
    /// See also [documentation](https://www.rabbitmq.com/queues.html#properties).
    pub fn with_auto_delete(mut self, auto_delete: bool) -> Self {
        self.options.auto_delete = auto_delete;
        self
    }

    /// Set the `durable` property of the queue (defaults to `false`).
    /// See also the [documentation](https://www.rabbitmq.com/queues.html#properties).
    pub fn with_durable(mut self, durable: bool) -> Self {
        self.options.durable = durable;
        self
    }

    /// Queues will expire after a period of time only when they are not used (e.g. do not have consumers).
    /// See [documentation](https://www.rabbitmq.com/ttl.html#queue-ttl).
    // Panic is extremely unlikely, let's not bother.
    #[allow(clippy::missing_panics_doc)]
    pub fn with_expires(mut self, expires: Duration) -> Self {
        let millis: u32 = expires
            .as_millis()
            .try_into()
            .expect("Duration too long to fit milliseconds in 32 bits");

        self.arguments.insert("x-expires".into(), millis.into());
        self
    }

    /// Messages expires if not consumed within `message_ttl`.
    /// See [documentation](https://www.rabbitmq.com/ttl.html#message-ttl-using-x-args).
    // Panic is extremely unlikely, let's not bother.
    #[allow(clippy::missing_panics_doc)]
    pub fn with_message_ttl(mut self, message_ttl: Duration) -> Self {
        let millis: u32 = message_ttl
            .as_millis()
            .try_into()
            .expect("Duration too long to fit milliseconds in 32 bits");

        self.arguments.insert("x-message-ttl".into(), millis.into());
        self
    }

    /// Sets the `x-dead-letter-exchange` argument on the queue. See also [RabbitMQ's documentation](https://www.rabbitmq.com/dlx.html).
    pub fn with_dead_letter_exchange(mut self, dead_letter_exchange: String) -> Self {
        self.arguments.insert(
            "x-dead-letter-exchange".into(),
            AMQPValue::LongString(dead_letter_exchange.into()),
        );
        self
    }

    /// Sets the `x-dead-letter-routing-key` argument on the queue. See also [RabbitMQ's documentation](https://www.rabbitmq.com/dlx.html).
    pub fn with_dead_letter_routing_key(mut self, dead_letter_routing_key: String) -> Self {
        self.arguments.insert(
            "x-dead-letter-routing-key".into(),
            AMQPValue::LongString(dead_letter_routing_key.into()),
        );
        self
    }

    /// Set any argument with any value.
    ///
    /// Prefer the more specific methods if you can, but you can use this for any specific argument you might want to set.
    pub fn with_arg(mut self, arg: impl Into<ShortString>, value: impl Into<AMQPValue>) -> Self {
        self.arguments.insert(arg.into(), value.into());
        self
    }

    /// Sets whether or not the handler should reply to messages. Defaults to true.
    pub fn with_replies(mut self, should_reply: bool) -> Self {
        self.should_reply = should_reply;
        self
    }
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            queue: None,
            exchange: Self::DIRECT_EXCHANGE.to_string(),
            prefetch: Self::DEFAULT_PREFETCH,
            options: QueueDeclareOptions {
                auto_delete: true,
                ..Default::default()
            },
            arguments: Default::default(),
            should_reply: true,
        }
    }
}
