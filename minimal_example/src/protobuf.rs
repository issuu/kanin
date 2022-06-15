pub mod generated;
pub use generated::*;

use self::echo::{
    echo_response::{Response, Success},
    EchoResponse,
};

impl EchoResponse {
    pub fn success(value: String) -> Self {
        Self {
            response: Some(Response::Success(Success { value })),
        }
    }
}
