pub mod client;
pub mod paths;
pub mod rpc;
pub mod types;

pub const PROTO_VERSION: u32 = 2;
/// The oldest version a hub still speaks with machines.
pub const PROTO_VERSION_PREVIOUS: u32 = PROTO_VERSION - 1;
pub const BUILD_ID: &str = env!("ASTERISM_BUILD");
