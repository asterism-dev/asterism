pub mod client;
pub mod paths;
pub mod rpc;
pub mod types;

pub const PROTO_VERSION: u32 = 1;
pub const BUILD_ID: &str = env!("ASTERISM_BUILD");
