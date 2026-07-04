use blake2b_simd::Params;
use sha2::{Digest, Sha256};
use tracing::debug;

/// Result of injecting a coinbase tag.
pub struct InjectionResult {
    /// The modified coinbase transaction hex.
    pub new_coinbase_hex: String,
    /// For v4: new txid (SHA256d of full tx). None for v5/v6.
    pub new_txid: Option<String>,
    /// For v5/v6: new hashBlockCommitments in RPC display byte order.
    /// None for v4.
    pub new_block_commitments: Option<String>,
}

/// Maximum scriptSig length we allow after tag injection.
const MAX_SCRIPT_SIG_LEN: usize = 100;

const V4_HEADER: [u8; 4] = [0x04, 0x00, 0x00, 0x80];
const V5_HEADER: [u8; 4] = [0x05, 0x00, 0x00, 0x80];
const V6_HEADER: [u8; 4] = [0x06, 0x00, 0x00, 0x80];

const ZCASH_BLOCK_COMMIT_PERSONALIZATION: &[u8; 16] = b"ZcashBlockCommit";
const ZCASH_AUTH_DATA_PERSONALIZATION: &[u8; 16] = b"ZcashAuthDatHash";
const ZCASH_AUTH_PERSONALIZATION_PREFIX: &[u8; 12] = b"ZTxAuthHash_";
const ZCASH_TRANSPARENT_AUTH_PERSONALIZATION: &[u8; 16] = b"ZTxAuthTransHash";
const ZCASH_SAPLING_AUTH_PERSONALIZATION: &[u8; 16] = b"ZTxAuthSapliHash";
const ZCASH_SAPLING_V6_AUTH_PERSONALIZATION: &[u8; 16] = b"ZTxAuthSapliH_v6";
const ZCASH_ORCHARD_AUTH_PERSONALIZATION: &[u8; 16] = b"ZTxAuthOrchaHash";
const ZCASH_ORCHARD_V6_AUTH_PERSONALIZATION: &[u8; 16] = b"ZTxAuthOrchaH_v6";
const ZCASH_IRONWOOD_AUTH_PERSONALIZATION: &[u8; 16] = b"ZTxAuthIrnwdH_v6";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TxVersion {
    V4,
    V5,
    V6,
}

impl TxVersion {
    fn from_header(header: &[u8]) -> Result<Self, String> {
        if header == V4_HEADER {
            Ok(Self::V4)
        } else if header == V5_HEADER {
            Ok(Self::V5)
        } else if header == V6_HEADER {
            Ok(Self::V6)
        } else {
            Err(format!(
                "Unknown tx version: {:02x}{:02x}{:02x}{:02x}",
                header[0], header[1], header[2], header[3]
            ))
        }
    }

    fn header_len(self) -> usize {
        match self {
            Self::V4 => 8,
            Self::V5 | Self::V6 => 20,
        }
    }

    fn has_zip244_auth(self) -> bool {
        matches!(self, Self::V5 | Self::V6)
    }
}

/// Inject a tag into the coinbase transaction's scriptSig.
///
/// For v5/v6 transactions, recomputes the auth digest chain and returns
/// a new `hashBlockCommitments`. The `tx_auth_digests` parameter must
/// contain the auth digests (hex, RPC byte order) of all non-coinbase
/// transactions in block order (from zebrad's `authdigest` field).
///
/// For v4, returns a new txid.
///
/// `chain_history_root_hex` is in RPC byte order (from zebrad's `defaultroots`).
pub fn inject_coinbase_tag(
    coinbase_hex: &str,
    tag: &[u8],
    chain_history_root_hex: &str,
    tx_auth_digests: &[String],
) -> Result<InjectionResult, String> {
    let data = hex::decode(coinbase_hex).map_err(|e| format!("Invalid coinbase hex: {e}"))?;

    if data.len() < 4 {
        return Err("Coinbase too short".into());
    }

    let version = TxVersion::from_header(&data[0..4])?;

    // Skip header to reach tx_in_count
    // v5/v6: version(4) + version_group_id(4) + consensus_branch_id(4)
    //        + lock_time(4) + expiry_height(4) = 20
    // v4: version(4) + version_group_id(4) = 8
    let header_len = version.header_len();

    if data.len() < header_len + 1 {
        return Err("Coinbase too short for header".into());
    }

    // Read tx_in_count (should be 1 for coinbase)
    let (tx_in_count, cs_len) = read_compact_size(&data, header_len)?;
    if tx_in_count != 1 {
        return Err(format!("Expected 1 txin in coinbase, got {tx_in_count}"));
    }

    // Skip prevout (32-byte hash + 4-byte index = 36 bytes)
    let prevout_offset = header_len + cs_len;
    let script_sig_len_offset = prevout_offset + 36;

    if data.len() < script_sig_len_offset + 1 {
        return Err("Coinbase too short for prevout".into());
    }

    // Read scriptSig length
    let (script_sig_len, sig_cs_len) = read_compact_size(&data, script_sig_len_offset)?;
    let script_sig_offset = script_sig_len_offset + sig_cs_len;
    let script_sig_end = script_sig_offset + script_sig_len;

    if data.len() < script_sig_end {
        return Err("Coinbase too short for scriptSig".into());
    }

    // Check if tag already present
    let existing_sig = &data[script_sig_offset..script_sig_end];
    if existing_sig.windows(tag.len()).any(|w| w == tag) {
        debug!("Coinbase tag already present, skipping injection");
        return Ok(InjectionResult {
            new_coinbase_hex: coinbase_hex.to_string(),
            new_txid: None,
            new_block_commitments: None,
        });
    }

    // Check length limit
    let new_script_sig_len = script_sig_len + tag.len();
    if new_script_sig_len > MAX_SCRIPT_SIG_LEN {
        return Err(format!(
            "scriptSig would be {} bytes (max {})",
            new_script_sig_len, MAX_SCRIPT_SIG_LEN
        ));
    }

    // Build new scriptSig: original + tag bytes
    let mut new_script_sig = data[script_sig_offset..script_sig_end].to_vec();
    new_script_sig.extend_from_slice(tag);

    // New compactSize for scriptSig length (always 1 byte since max is 100)
    let new_cs = compact_size_byte(new_script_sig_len)?;

    // Reconstruct the transaction:
    // [everything before scriptSig length] + [new_cs] + [new_scriptSig] + [rest after old scriptSig]
    let mut new_data = Vec::with_capacity(data.len() + tag.len());
    new_data.extend_from_slice(&data[..script_sig_len_offset]);
    new_data.push(new_cs);
    new_data.extend_from_slice(&new_script_sig);
    new_data.extend_from_slice(&data[script_sig_end..]);

    if version.has_zip244_auth() {
        // Extract consensus_branch_id from coinbase bytes 8-11
        let consensus_branch_id = &data[8..12];

        // Compute new coinbase auth_digest (internal byte order)
        let coinbase_auth =
            compute_coinbase_auth_digest(consensus_branch_id, version, new_cs, &new_script_sig);

        // Parse non-coinbase tx auth digests from template.
        // Zebrad returns auth digests in RPC byte order (reversed); we need internal order.
        let mut leaves = Vec::with_capacity(1 + tx_auth_digests.len());
        leaves.push(coinbase_auth);
        for (i, ad_hex) in tx_auth_digests.iter().enumerate() {
            let ad_bytes = hex::decode(ad_hex)
                .map_err(|e| format!("Invalid authdigest hex for tx {i}: {e}"))?;
            if ad_bytes.len() != 32 {
                return Err(format!(
                    "authdigest for tx {i} is {} bytes, expected 32",
                    ad_bytes.len()
                ));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&ad_bytes);
            arr.reverse(); // RPC order → internal order
            leaves.push(arr);
        }

        // Build auth data merkle root (internal byte order)
        let auth_data_root = auth_data_merkle_root(&leaves);

        // chain_history_root from zebrad is in RPC byte order (reversed); reverse to internal.
        let mut chain_history_root = hex::decode(chain_history_root_hex)
            .map_err(|e| format!("Invalid chain_history_root hex: {e}"))?;
        if chain_history_root.len() != 32 {
            return Err(format!(
                "chain_history_root must be 32 bytes, got {}",
                chain_history_root.len()
            ));
        }
        chain_history_root.reverse(); // RPC order → internal order

        // block_commitments = BLAKE2b("ZcashBlockCommit", chain_history_root || auth_data_root || [0; 32])
        let mut commit_input = [0u8; 96];
        commit_input[..32].copy_from_slice(&chain_history_root);
        commit_input[32..64].copy_from_slice(&auth_data_root);
        // commit_input[64..96] already zero (terminator)
        let mut block_commitments = blake2b_256(ZCASH_BLOCK_COMMIT_PERSONALIZATION, &commit_input);
        block_commitments.reverse(); // internal order → RPC order

        Ok(InjectionResult {
            new_coinbase_hex: hex::encode(&new_data),
            new_txid: None,
            new_block_commitments: Some(hex::encode(block_commitments)),
        })
    } else {
        // v4: txid = SHA256d of the full serialized tx (displayed in reverse byte order)
        let txid_bytes = sha256d(&new_data);
        let txid_hex = hex::encode(txid_bytes.iter().rev().copied().collect::<Vec<u8>>());

        Ok(InjectionResult {
            new_coinbase_hex: hex::encode(&new_data),
            new_txid: Some(txid_hex),
            new_block_commitments: None,
        })
    }
}

/// Compute the auth_digest of a coinbase transaction after scriptSig modification.
///
/// Returns the auth_digest in INTERNAL byte order (not RPC display order).
///
/// Empty bundle auth digests are version-specific BLAKE2b hashes, not `[0;32]`.
fn compute_coinbase_auth_digest(
    consensus_branch_id: &[u8],
    version: TxVersion,
    script_sig_cs: u8,
    new_script_sig: &[u8],
) -> [u8; 32] {
    // transparent_scripts_digest = BLAKE2b("ZTxAuthTransHash", compactSize(len) || scriptSig)
    let mut scripts_input = Vec::with_capacity(1 + new_script_sig.len());
    scripts_input.push(script_sig_cs);
    scripts_input.extend_from_slice(new_script_sig);
    let transparent_scripts_digest =
        blake2b_256(ZCASH_TRANSPARENT_AUTH_PERSONALIZATION, &scripts_input);

    // Coinbase has no shielded bundles, but the auth digests are NOT zero.
    // They are BLAKE2b hashes of empty data with versioned personalizations.
    let (sapling_personal, orchard_personal) = match version {
        TxVersion::V4 => unreachable!("v4 transactions do not have ZIP-244 auth digests"),
        TxVersion::V5 => (
            ZCASH_SAPLING_AUTH_PERSONALIZATION.as_slice(),
            ZCASH_ORCHARD_AUTH_PERSONALIZATION.as_slice(),
        ),
        TxVersion::V6 => (
            ZCASH_SAPLING_V6_AUTH_PERSONALIZATION.as_slice(),
            ZCASH_ORCHARD_V6_AUTH_PERSONALIZATION.as_slice(),
        ),
    };
    let sapling_auth_digest = blake2b_256(sapling_personal, &[]);
    let orchard_auth_digest = blake2b_256(orchard_personal, &[]);
    let ironwood_auth_digest =
        (version == TxVersion::V6).then(|| blake2b_256(ZCASH_IRONWOOD_AUTH_PERSONALIZATION, &[]));

    // auth_digest = BLAKE2b("ZTxAuthHash_" || branch_id,
    //                       transparent || sapling || orchard [|| ironwood])
    let mut auth_perso = [0u8; 16];
    auth_perso[..12].copy_from_slice(ZCASH_AUTH_PERSONALIZATION_PREFIX);
    auth_perso[12..16].copy_from_slice(consensus_branch_id);

    let auth_input_len = if ironwood_auth_digest.is_some() {
        128
    } else {
        96
    };
    let mut auth_input = Vec::with_capacity(auth_input_len);
    auth_input.extend_from_slice(&transparent_scripts_digest);
    auth_input.extend_from_slice(&sapling_auth_digest);
    auth_input.extend_from_slice(&orchard_auth_digest);
    if let Some(ironwood_auth_digest) = ironwood_auth_digest {
        auth_input.extend_from_slice(&ironwood_auth_digest);
    }
    blake2b_256(&auth_perso, &auth_input)
}

/// Compute the auth data merkle root from a list of transaction auth digests
/// (in internal byte order).
///
/// Uses a perfect binary tree padded with [0; 32] to the next power of 2.
/// For 1 leaf, the root IS the leaf (no padding needed).
/// Hash function: BLAKE2b-256 with personalization "ZcashAuthDatHash".
fn auth_data_merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    // Pad to next power of 2 with zero leaves.
    // For 1 leaf, next_power_of_two() == 1 so no padding — root IS the leaf.
    let n = leaves.len().next_power_of_two();
    let mut current: Vec<[u8; 32]> = Vec::with_capacity(n);
    current.extend_from_slice(leaves);
    current.resize(n, [0u8; 32]);

    // Build tree bottom-up
    while current.len() > 1 {
        let mut next = Vec::with_capacity(current.len() / 2);
        for pair in current.chunks(2) {
            let mut input = [0u8; 64];
            input[..32].copy_from_slice(&pair[0]);
            input[32..64].copy_from_slice(&pair[1]);
            next.push(blake2b_256(ZCASH_AUTH_DATA_PERSONALIZATION, &input));
        }
        current = next;
    }

    current[0]
}

/// Read a compactSize integer at the given offset. Returns (value, bytes_consumed).
fn read_compact_size(data: &[u8], offset: usize) -> Result<(usize, usize), String> {
    if offset >= data.len() {
        return Err("read_compact_size: offset beyond data".into());
    }
    match data[offset] {
        0..=252 => Ok((data[offset] as usize, 1)),
        0xFD => {
            if offset + 3 > data.len() {
                return Err("Truncated compactSize (FD)".into());
            }
            let val = u16::from_le_bytes([data[offset + 1], data[offset + 2]]) as usize;
            Ok((val, 3))
        }
        0xFE => {
            if offset + 5 > data.len() {
                return Err("Truncated compactSize (FE)".into());
            }
            let val = u32::from_le_bytes([
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
            ]) as usize;
            Ok((val, 5))
        }
        0xFF => {
            if offset + 9 > data.len() {
                return Err("Truncated compactSize (FF)".into());
            }
            let val = u64::from_le_bytes(data[offset + 1..offset + 9].try_into().unwrap()) as usize;
            Ok((val, 9))
        }
    }
}

/// Encode a value as a 1-byte compactSize. Errors if value >= 253.
fn compact_size_byte(n: usize) -> Result<u8, String> {
    if n >= 253 {
        return Err(format!("compact_size_byte: value {n} too large for 1 byte"));
    }
    Ok(n as u8)
}

/// BLAKE2b-256 with a 16-byte personalization string.
fn blake2b_256(personalization: &[u8], data: &[u8]) -> [u8; 32] {
    let hash = Params::new()
        .hash_length(32)
        .personal(personalization)
        .hash(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    out
}

/// Reverse a hex string's byte order (e.g. "aabb" -> "bbaa").
fn _reverse_hex(hex_str: &str) -> String {
    let bytes = hex::decode(hex_str).unwrap_or_default();
    let reversed: Vec<u8> = bytes.into_iter().rev().collect();
    hex::encode(reversed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_compact_size() {
        assert_eq!(read_compact_size(&[42], 0).unwrap(), (42, 1));
        assert_eq!(read_compact_size(&[252], 0).unwrap(), (252, 1));
        let mut data = vec![0xFD, 0x2C, 0x01];
        assert_eq!(read_compact_size(&data, 0).unwrap(), (300, 3));
        data.insert(0, 0xFF);
        assert_eq!(read_compact_size(&data, 1).unwrap(), (300, 3));
    }

    #[test]
    fn test_compact_size_byte() {
        assert_eq!(compact_size_byte(0).unwrap(), 0);
        assert_eq!(compact_size_byte(100).unwrap(), 100);
        assert_eq!(compact_size_byte(252).unwrap(), 252);
        assert!(compact_size_byte(253).is_err());
    }

    #[test]
    fn test_auth_data_merkle_root_single() {
        // 1 leaf → no padding, root IS the leaf itself
        let leaf = [0xAA; 32];
        let root = auth_data_merkle_root(&[leaf]);
        assert_eq!(root, leaf);
    }

    #[test]
    fn test_auth_data_merkle_root_two() {
        let leaf0 = [0xAA; 32];
        let leaf1 = [0xBB; 32];
        let root = auth_data_merkle_root(&[leaf0, leaf1]);

        let mut expected_input = [0u8; 64];
        expected_input[..32].copy_from_slice(&leaf0);
        expected_input[32..64].copy_from_slice(&leaf1);
        let expected = blake2b_256(ZCASH_AUTH_DATA_PERSONALIZATION, &expected_input);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_auth_data_merkle_root_three() {
        let l0 = [0xAA; 32];
        let l1 = [0xBB; 32];
        let l2 = [0xCC; 32];
        let root = auth_data_merkle_root(&[l0, l1, l2]);

        let mut input01 = [0u8; 64];
        input01[..32].copy_from_slice(&l0);
        input01[32..64].copy_from_slice(&l1);
        let h01 = blake2b_256(ZCASH_AUTH_DATA_PERSONALIZATION, &input01);

        let mut input2z = [0u8; 64];
        input2z[..32].copy_from_slice(&l2);
        let h2z = blake2b_256(ZCASH_AUTH_DATA_PERSONALIZATION, &input2z);

        let mut input_top = [0u8; 64];
        input_top[..32].copy_from_slice(&h01);
        input_top[32..64].copy_from_slice(&h2z);
        let expected = blake2b_256(ZCASH_AUTH_DATA_PERSONALIZATION, &input_top);

        assert_eq!(root, expected);
    }

    /// End-to-end test using real data from zebrad's getblocktemplate.
    /// Verifies auth_digest and blockcommitmentshash match zebrad's values.
    #[test]
    fn test_against_real_template() {
        // Real data from zebrad testnet getblocktemplate (height 3902854, 0 txs)
        let coinbase_hex = "050000800a27a726f04dec4d00000000868d3b00010000000000000000000000000000000000000000000000000000000000000000ffffffff0903868d3b7af09fa693000000000240597307000000001976a9143f1d707eae9297983695aa5dbf983e03b638530c88ac20bcbe000000000017a9147a86d6c7eb12ce0aa309d7391a6f338eba3c242b87000000";
        let chain_history_root = "15a4875bc1c8555d4f9ee74798d796c5a39ad6934d5096048cd9653442822a2e";
        // zebrad's authdigest and blockcommitmentshash are in RPC byte order (reversed)
        let expected_authdigest_rpc =
            "31ffddbecc7bc45a3d182f52ed356128b96be69ddff221f18d8959a3b5cd20df";
        let expected_blockcommitments_rpc =
            "782c59d40904570b53ee60d13f89597f0c942d78a364e147b61c33b996dcfd62";

        let data = hex::decode(coinbase_hex).unwrap();
        let consensus_branch_id = &data[8..12]; // f04dec4d

        // Parse scriptSig
        let header_len = 20;
        let (_tx_in_count, cs_len) = read_compact_size(&data, header_len).unwrap();
        let script_sig_len_offset = header_len + cs_len + 36;
        let (script_sig_len, sig_cs_len) = read_compact_size(&data, script_sig_len_offset).unwrap();
        let script_sig_offset = script_sig_len_offset + sig_cs_len;
        let script_sig = &data[script_sig_offset..script_sig_offset + script_sig_len];
        let script_sig_cs = script_sig_len as u8;

        // Verify coinbase auth_digest (internal order, reversed = RPC order)
        let auth = compute_coinbase_auth_digest(
            consensus_branch_id,
            TxVersion::V5,
            script_sig_cs,
            script_sig,
        );
        let auth_rpc: Vec<u8> = auth.iter().rev().copied().collect();
        assert_eq!(hex::encode(&auth_rpc), expected_authdigest_rpc);

        // auth_data_root for 1 tx = the auth_digest itself
        let root = auth_data_merkle_root(&[auth]);
        assert_eq!(root, auth);

        // blockcommitmentshash: chr and result are in RPC order (reversed)
        let mut chr = hex::decode(chain_history_root).unwrap();
        chr.reverse(); // RPC → internal
        let mut commit_input = [0u8; 96];
        commit_input[..32].copy_from_slice(&chr);
        commit_input[32..64].copy_from_slice(&root);
        let mut bc = blake2b_256(ZCASH_BLOCK_COMMIT_PERSONALIZATION, &commit_input);
        bc.reverse(); // internal → RPC
        assert_eq!(hex::encode(bc), expected_blockcommitments_rpc);
    }

    #[test]
    fn v6_coinbase_tag_updates_block_commitments() {
        // v6 uses the same transparent coinbase layout position as v5, but
        // has a different version group ID and NU6.3 consensus branch ID.
        let coinbase_hex = "0600008098b684d85b16a53700000000868d3b00010000000000000000000000000000000000000000000000000000000000000000ffffffff0903868d3b7af09fa693000000000240597307000000001976a9143f1d707eae9297983695aa5dbf983e03b638530c88ac20bcbe000000000017a9147a86d6c7eb12ce0aa309d7391a6f338eba3c242b8700000000";
        let chain_history_root = "15a4875bc1c8555d4f9ee74798d796c5a39ad6934d5096048cd9653442822a2e";
        let result = inject_coinbase_tag(coinbase_hex, b"zkclaudecoder", chain_history_root, &[])
            .expect("v6 coinbase tag injection succeeds");

        assert!(result.new_txid.is_none());
        assert!(result
            .new_coinbase_hex
            .contains("7af09fa6937a6b636c61756465636f646572"));
        assert_eq!(
            result.new_block_commitments.as_deref(),
            Some("1dcafaa83b1a39a9f4c5eedd070b48184ddc651668fed3ced77c97ec8941a1cb")
        );
    }

    #[test]
    fn v6_coinbase_auth_digest_uses_v6_empty_bundle_hashes() {
        let script_sig = hex::decode("03868d3b7af09fa6937a6b636c61756465636f646572").unwrap();
        let auth = compute_coinbase_auth_digest(
            &0x37a5165b_u32.to_le_bytes(),
            TxVersion::V6,
            script_sig.len() as u8,
            &script_sig,
        );
        let auth_rpc: Vec<u8> = auth.iter().rev().copied().collect();

        assert_eq!(
            hex::encode(auth_rpc),
            "d93bc7c4bfa13ea7a88cb3b52f35b516280eb17eba227f9d575091301bcc1da1"
        );
    }
}
