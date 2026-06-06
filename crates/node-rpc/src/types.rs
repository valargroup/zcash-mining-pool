use serde::{Deserialize, Serialize};

/// A transaction included in a block template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplateTransaction {
    pub data: String,
    pub hash: String,
    pub fee: i64,
    pub sigops: u64,
    #[serde(default)]
    pub required: bool,
    /// Authorizing data digest (v5 transactions). Provided by zebrad.
    #[serde(default)]
    pub authdigest: Option<String>,
}

/// Response from the `getblocktemplate` RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub version: u32,
    pub previousblockhash: String,
    /// Light-client root commitment (NU5+).
    #[serde(default)]
    pub lightclientroothash: Option<String>,
    /// Block commitments hash (NU5+).
    #[serde(default)]
    pub blockcommitmentshash: Option<String>,
    /// Final sapling root hash.
    #[serde(default)]
    pub finalsaplingroothash: Option<String>,
    /// Default root hashes computed by the node.
    #[serde(default)]
    pub defaultroots: Option<DefaultRoots>,
    pub transactions: Vec<BlockTemplateTransaction>,
    /// Coinbase transaction fields.
    #[serde(default)]
    pub coinbasetxn: Option<CoinbaseTxn>,
    pub target: String,
    pub mintime: u64,
    pub curtime: u64,
    pub bits: String,
    pub height: u64,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub mutable: Vec<String>,
    #[serde(default)]
    pub noncerange: Option<String>,
    #[serde(default)]
    pub sigoplimit: Option<u64>,
    #[serde(default)]
    pub sizelimit: Option<u64>,
    /// BIP22 long-polling identifier. Opaque string returned by the node
    /// that identifies this template version. Clients can pass it back to
    /// request a long-polled `getblocktemplate` that blocks until the
    /// template changes.
    #[serde(default)]
    pub longpollid: Option<String>,
}

/// Default root hashes returned by getblocktemplate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRoots {
    #[serde(default)]
    pub merkleroot: Option<String>,
    #[serde(default)]
    pub chainhistoryroot: Option<String>,
    #[serde(default)]
    pub authdataroot: Option<String>,
    #[serde(default)]
    pub blockcommitmentshash: Option<String>,
}

/// Coinbase transaction info from the block template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseTxn {
    pub data: String,
    pub hash: String,
    #[serde(default)]
    pub fee: Option<i64>,
    #[serde(default)]
    pub sigops: Option<u64>,
    #[serde(default)]
    pub required: Option<bool>,
}

impl CoinbaseTxn {
    /// Count transparent outputs in the serialized coinbase transaction.
    pub fn transparent_output_count(&self) -> Result<usize, String> {
        transparent_output_count_from_tx_hex(&self.data)
    }
}

fn transparent_output_count_from_tx_hex(tx_hex: &str) -> Result<usize, String> {
    let data = hex::decode(tx_hex).map_err(|e| format!("invalid coinbase hex: {e}"))?;
    if data.len() < 4 {
        return Err("coinbase transaction is too short".to_string());
    }

    let header_len = match data[0..4] {
        [0x05, 0x00, 0x00, 0x80] => 20,
        [0x04, 0x00, 0x00, 0x80] => 8,
        _ => {
            return Err(format!(
                "unsupported coinbase transaction version {:02x}{:02x}{:02x}{:02x}",
                data[0], data[1], data[2], data[3]
            ));
        }
    };

    if data.len() < header_len {
        return Err("coinbase transaction is shorter than its header".to_string());
    }

    let (input_count, input_count_len) = read_compact_size(&data, header_len)?;
    let output_count_offset =
        skip_transparent_inputs(&data, header_len + input_count_len, input_count)?;
    let (output_count, _) = read_compact_size(&data, output_count_offset)?;
    Ok(output_count)
}

fn skip_transparent_inputs(
    data: &[u8],
    mut offset: usize,
    input_count: usize,
) -> Result<usize, String> {
    for _ in 0..input_count {
        let script_len_offset = offset
            .checked_add(36)
            .ok_or_else(|| "transparent input offset overflow".to_string())?;
        if data.len() < script_len_offset {
            return Err("coinbase transaction is truncated before input script".to_string());
        }

        let (script_len, script_len_size) = read_compact_size(data, script_len_offset)?;
        let script_start = script_len_offset
            .checked_add(script_len_size)
            .ok_or_else(|| "input script offset overflow".to_string())?;
        let script_end = script_start
            .checked_add(script_len)
            .ok_or_else(|| "input script length overflow".to_string())?;
        offset = script_end
            .checked_add(4)
            .ok_or_else(|| "transparent input sequence offset overflow".to_string())?;
        if data.len() < offset {
            return Err("coinbase transaction is truncated inside input".to_string());
        }
    }

    Ok(offset)
}

fn read_compact_size(data: &[u8], offset: usize) -> Result<(usize, usize), String> {
    if offset >= data.len() {
        return Err("compactSize offset is past the end of the transaction".to_string());
    }

    match data[offset] {
        0..=252 => Ok((data[offset] as usize, 1)),
        0xFD => {
            if offset + 3 > data.len() {
                return Err("truncated compactSize u16".to_string());
            }
            Ok((
                u16::from_le_bytes([data[offset + 1], data[offset + 2]]) as usize,
                3,
            ))
        }
        0xFE => {
            if offset + 5 > data.len() {
                return Err("truncated compactSize u32".to_string());
            }
            Ok((
                u32::from_le_bytes([
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                ]) as usize,
                5,
            ))
        }
        0xFF => {
            if offset + 9 > data.len() {
                return Err("truncated compactSize u64".to_string());
            }
            let value = u64::from_le_bytes([
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
                data[offset + 8],
            ]);
            usize::try_from(value)
                .map(|v| (v, 9))
                .map_err(|_| "compactSize value does not fit usize".to_string())
        }
    }
}

/// Response from `getblockcount`.
pub type BlockCount = u64;

/// Response from `getbestblockhash`.
pub type BestBlockHash = String;

/// Response from `getinfo` (subset of fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    #[serde(default)]
    pub version: Option<u64>,
    #[serde(default)]
    pub subversion: Option<String>,
    #[serde(default)]
    pub blocks: Option<u64>,
    #[serde(default)]
    pub connections: Option<u64>,
    #[serde(default)]
    pub testnet: Option<bool>,
}

/// Generic JSON-RPC request envelope.
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest<'a> {
    pub jsonrpc: &'a str,
    pub id: u64,
    pub method: &'a str,
    pub params: serde_json::Value,
}

/// Generic JSON-RPC response envelope.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub id: Option<u64>,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

#[cfg(test)]
mod tests {
    use super::CoinbaseTxn;

    fn v5_coinbase_with_output_count(output_count: u8) -> CoinbaseTxn {
        let mut bytes = vec![
            0x05, 0x00, 0x00, 0x80, // v5 overwintered
            0x0a, 0x27, 0xa7, 0x26, // version group id
            0xc8, 0xe7, 0x10, 0x55, // consensus branch id
            0x00, 0x00, 0x00, 0x00, // lock time
            0x00, 0x00, 0x00, 0x00, // expiry height
            0x01, // one transparent input
        ];
        bytes.extend_from_slice(&[0; 32]);
        bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        bytes.push(1);
        bytes.push(0);
        bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        bytes.push(output_count);

        CoinbaseTxn {
            data: hex::encode(bytes),
            hash: String::new(),
            fee: None,
            sigops: None,
            required: None,
        }
    }

    #[test]
    fn counts_zero_transparent_outputs() {
        assert_eq!(
            v5_coinbase_with_output_count(0).transparent_output_count(),
            Ok(0)
        );
    }

    #[test]
    fn counts_existing_transparent_outputs() {
        assert_eq!(
            v5_coinbase_with_output_count(2).transparent_output_count(),
            Ok(2)
        );
    }
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RPC error {}: {}", self.code, self.message)
    }
}
