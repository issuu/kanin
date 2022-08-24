use kanin::{extract::Msg, App};

use self::generated::{
    echo_response::{Result, Success},
    *,
};

async fn proto_handler(Msg(request): Msg<EchoRequest>) -> EchoResponse {
    EchoResponse {
        result: Some(Result::Success(Success {
            value: request.value,
        })),
    }
}

#[tokio::test]
async fn it_compiles() {
    let _ignore = App::new()
        .handler("routing_key", proto_handler)
        .run("amqp_addr")
        .await;
}

#[allow(clippy::derive_partial_eq_without_eq)]
mod generated {
    //! Normally this would be generated by prost but we'll just write it directly for the purposes of this test.

    use kanin::FromError;

    /// An internal error. This is used for any error that can't be handled in any other way.
    /// Consider it a last resort when no other more specific error can be returned.
    #[derive(FromError, Clone, PartialEq, ::prost::Message)]
    pub struct InternalError {
        /// The source is an a1pp ID that specifies the service in which the error originated.
        #[prost(string, tag = "1")]
        pub source: ::prost::alloc::string::String,
        /// Description of the error.
        #[prost(string, tag = "2")]
        pub error: ::prost::alloc::string::String,
    }
    /// An invalid request.
    #[derive(FromError, Clone, PartialEq, ::prost::Message)]
    pub struct InvalidRequest {
        /// Description of how the request was invalid.
        #[prost(string, tag = "1")]
        pub error: ::prost::alloc::string::String,
    }
    /// The request for the echo handler.
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct EchoRequest {
        /// The value to echo back to the caller.
        #[prost(string, tag = "1")]
        pub value: ::prost::alloc::string::String,
    }
    /// The echo handler will respond with this message.
    #[derive(FromError, Clone, PartialEq, ::prost::Message)]
    pub struct EchoResponse {
        /// The result of the request must be one of the following variants.
        #[prost(oneof = "echo_response::Result", tags = "1, 2")]
        pub result: ::core::option::Option<echo_response::Result>,
    }
    /// Nested message and enum types in `EchoResponse`.
    pub mod echo_response {
        use kanin_derive::FromError;

        /// Success variant of the response.
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Success {
            /// The same string that was given by the caller.
            /// The yell endpoint will have it capitalized.
            #[prost(string, tag = "1")]
            pub value: ::prost::alloc::string::String,
        }
        /// The result of the request must be one of the following variants.
        #[derive(FromError, Clone, PartialEq, ::prost::Oneof)]
        pub enum Result {
            /// The request was successful.
            #[prost(message, tag = "1")]
            Success(Success),
            /// The request was invalid.
            #[prost(message, tag = "2")]
            InvalidRequest(super::InvalidRequest),
            /// The request resulted in an unhandled error.
            #[prost(message, tag = "3")]
            InternalError(super::InternalError),
        }
    }
}
