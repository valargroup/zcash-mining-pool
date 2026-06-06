use chrono::Utc;
use pool_db::PendingPayout;
use tracing::warn;

use crate::address::is_valid_zcash_address;
use crate::handlers::CoinbasePayoutMode;

/// A pending payout row that can be included in a `z_sendmany` batch.
#[derive(Debug, Clone)]
pub struct PayablePayout {
    pub pending_index: usize,
    pub amount_zatoshis: i64,
    pub pay_to: String,
}

/// Summary of pending payout rows excluded or redirected during selection.
#[derive(Debug, Clone, Copy, Default)]
pub struct PayoutSelectionStats {
    pub held_invalid: usize,
    pub skipped_invalid: usize,
    pub redirected: usize,
}

/// Filters pending payout rows into the addresses that are safe to pass to `z_sendmany`.
///
/// In direct shielded mode, invalid stale miner addresses are held instead of
/// redirected to the pool source address. In transparent mode, stale invalid
/// addresses can still fall back to the configured mining address.
pub fn select_payable_payouts(
    pending: &[PendingPayout],
    mining_address: Option<&str>,
    coinbase_payout_mode: CoinbasePayoutMode,
    network: &str,
) -> (Vec<PayablePayout>, PayoutSelectionStats) {
    let mut payable = Vec::new();
    let mut stats = PayoutSelectionStats::default();
    let two_days_ago = Utc::now()
        .checked_sub_signed(chrono::Duration::days(2))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    for (pending_index, p) in pending.iter().enumerate() {
        let pay_to = if is_valid_zcash_address(&p.address, network) {
            p.address.clone()
        } else if p.created_at <= two_days_ago {
            if coinbase_payout_mode == CoinbasePayoutMode::DirectShielded {
                stats.held_invalid += 1;
                warn!(
                    miner_id = p.miner_id, address = %p.address, created_at = %p.created_at,
                    amount_zec = p.amount as f64 / 100_000_000.0,
                    "Holding payout: invalid address format (direct shielded mode has no non-source fallback)"
                );
                continue;
            }

            match mining_address {
                Some(mining_address) => {
                    stats.redirected += 1;
                    warn!(
                        miner_id = p.miner_id, address = %p.address, created_at = %p.created_at,
                        amount_zec = p.amount as f64 / 100_000_000.0,
                        "Redirecting payout to mining address (unpayable address, >2 days old)"
                    );
                    mining_address.to_string()
                }
                None => {
                    stats.held_invalid += 1;
                    warn!(
                        miner_id = p.miner_id, address = %p.address, created_at = %p.created_at,
                        amount_zec = p.amount as f64 / 100_000_000.0,
                        "Holding payout: invalid address format (no fallback mining address configured)"
                    );
                    continue;
                }
            }
        } else {
            stats.skipped_invalid += 1;
            warn!(
                miner_id = p.miner_id, address = %p.address,
                "Skipping payout: invalid address format (account < 2 days old)"
            );
            continue;
        };

        payable.push(PayablePayout {
            pending_index,
            amount_zatoshis: p.amount,
            pay_to,
        });
    }

    (payable, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(address: &str, created_at: &str, amount: i64) -> PendingPayout {
        PendingPayout {
            miner_id: 7,
            address: address.to_string(),
            amount,
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn direct_mode_holds_stale_invalid_addresses() {
        let pending = vec![pending("not-an-address", "2000-01-01 00:00:00", 50_000)];

        let (payable, stats) = select_payable_payouts(
            &pending,
            Some("utest1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"),
            CoinbasePayoutMode::DirectShielded,
            "testnet",
        );

        assert!(payable.is_empty());
        assert_eq!(stats.held_invalid, 1);
        assert_eq!(stats.redirected, 0);
    }

    #[test]
    fn transparent_mode_redirects_stale_invalid_addresses() {
        let mining_address = "utest1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
        let pending = vec![pending("not-an-address", "2000-01-01 00:00:00", 50_000)];

        let (payable, stats) = select_payable_payouts(
            &pending,
            Some(mining_address),
            CoinbasePayoutMode::Transparent,
            "testnet",
        );

        assert_eq!(payable.len(), 1);
        assert_eq!(payable[0].pay_to, mining_address);
        assert_eq!(stats.redirected, 1);
    }
}
