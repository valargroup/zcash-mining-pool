use std::sync::Arc;

use node_rpc::ZcashRpcClient;
use tracing::{error, info, warn};

pub struct BlockAssembler {
    rpc: Arc<ZcashRpcClient>,
}

impl BlockAssembler {
    pub fn new(rpc: Arc<ZcashRpcClient>) -> Self {
        Self { rpc }
    }

    pub async fn submit_block(&self, block_hex: &str) -> Result<(), BlockSubmitError> {
        info!(
            block_hex_len = block_hex.len(),
            block_bytes = block_hex.len() / 2,
            header_preview = &block_hex[..std::cmp::min(216, block_hex.len())],
            "Submitting solved block to network"
        );

        // First try proposal mode for detailed validation
        match self.rpc.validate_block_proposal(block_hex).await {
            Ok(result) => {
                if let Some(reject) = &result {
                    warn!(reason = %reject, "Block proposal rejected (pre-check)");
                } else {
                    info!("Block proposal validated OK");
                }
            }
            Err(e) => {
                // Proposal mode may not be supported; log and continue
                warn!(error = %e, "Block proposal validation failed (may not be supported)");
            }
        }

        match self.rpc.submit_block(block_hex).await {
            Ok(result) => {
                if let Some(reject_reason) = result {
                    if reject_reason.is_empty() || reject_reason == "null" {
                        info!("Block accepted by network!");
                        Ok(())
                    } else {
                        error!(reason = %reject_reason, "Block rejected by network");
                        Err(BlockSubmitError::Rejected(reject_reason))
                    }
                } else {
                    info!("Block accepted by network!");
                    Ok(())
                }
            }
            Err(e) => {
                error!(error = %e, "Block submission RPC failed");
                Err(BlockSubmitError::Rpc(e))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlockSubmitError {
    #[error("Block rejected: {0}")]
    Rejected(String),
    #[error("RPC error: {0}")]
    Rpc(#[from] node_rpc::RpcError),
}
