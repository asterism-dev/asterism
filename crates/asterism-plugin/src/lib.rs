pub mod protocol;
mod server;

pub use asterism_proto::rpc::{ErrorKind, RpcError};
pub use server::{serve, serve_io, Host};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// Decodes request params; `null` counts as an empty object.
pub fn params<T: DeserializeOwned>(value: Value) -> Result<T, RpcError> {
    let value = if value.is_null() { Value::Object(Default::default()) } else { value };
    serde_json::from_value(value).map_err(|e| RpcError::new(ErrorKind::InvalidParams, e.to_string()))
}

pub fn to_value<T: Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(ErrorKind::Internal, e.to_string()))
}
