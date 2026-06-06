pub mod client;
pub mod types;

pub use client::{
    RpcError, ZcashRpcClient, GET_BLOCK_TEMPLATE_TIMEOUT, GET_BLOCK_TEMPLATE_TIMEOUT_SECS,
};
pub use types::BlockTemplate;
