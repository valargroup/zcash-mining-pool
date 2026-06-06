/// Validates a Zcash address for the given network using encoding checks.
///
/// Transparent addresses use base58check encoding with network-specific
/// version bytes. Sapling and unified addresses use their expected HRP and
/// character set.
pub fn is_valid_zcash_address(addr: &str, network: &str) -> bool {
    let is_mainnet = network == "mainnet";

    if addr.starts_with('t') {
        let valid_prefix = if is_mainnet {
            addr.starts_with("t1") || addr.starts_with("t3")
        } else {
            addr.starts_with("tm") || addr.starts_with("t2")
        };
        if !valid_prefix {
            return false;
        }
        return match bs58::decode(addr).with_check(None).into_vec() {
            Ok(bytes) => bytes.len() == 22,
            Err(_) => false,
        };
    }

    if addr.starts_with('z') {
        let valid_prefix = if is_mainnet {
            addr.starts_with("zs1")
        } else {
            addr.starts_with("ztestsapling1")
        };
        if !valid_prefix {
            return false;
        }
        let hrp_end = addr.rfind('1').unwrap_or(0);
        let data_part = &addr[hrp_end + 1..];
        let valid_charset = data_part
            .chars()
            .all(|c| "qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(c));
        let expected_len = if is_mainnet { 78 } else { 88 };
        return valid_charset && addr.len() == expected_len;
    }

    if addr.starts_with('u') {
        let valid_prefix = if is_mainnet {
            addr.starts_with("u1") && !addr.starts_with("utest")
        } else {
            addr.starts_with("utest1")
        };
        if !valid_prefix {
            return false;
        }
        let hrp_end = addr.find('1').unwrap_or(0);
        let data_part = &addr[hrp_end + 1..];
        let valid_charset = data_part
            .chars()
            .all(|c| "qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(c));
        return valid_charset && data_part.len() >= 50 && addr.len() <= 320;
    }

    false
}
