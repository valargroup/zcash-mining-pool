# Wallet (Zallet) Status

The pool communicates with Zallet **only via RPC**. The pool does not start, stop, or manage the Zallet process. Zallet must be run externally (CLI, systemd, launchd, or a separate manager). For an AI-managed Zallet workspace, see the sibling **ZalletD** folder.

## Configuration

Wallet RPC is configured in `[payout]`:

```toml
[payout]
wallet_rpc_url = "http://127.0.0.1:28232"
wallet_rpc_user = "your_wallet_user"
wallet_rpc_password = "your_wallet_password"
# ... other payout options
```

`wallet_rpc_url` (and optionally `wallet_rpc_user`, `wallet_rpc_password`) is the only wallet-related configuration. The pool uses this to call `z_gettotalbalance` for status and balance, and `z_sendmany` / `z_getoperationstatus` for payouts.

## Dashboard (Human)

- **URL:** `http://<api_listen_addr>/zallet`
- **Main pool:** Link in header as "Wallet"

Shows (read-only):
- RPC status (OK / not ready)
- Wallet balance (transparent, private, total)
- Polls `/api/zallet/status` every 5 seconds

## API (Machine-Readable)

### GET /api/zallet/status

Returns JSON:

```json
{
  "rpc_ok": true,
  "balance": {
    "transparent": "1000.00000000",
    "private": "0.00000000",
    "total": "1000.00000000"
  },
  "error": null
}
```

- `rpc_ok`: Wallet RPC responds (`z_gettotalbalance` succeeds)
- `balance`: Transparent, private, and total ZEC
- `error`: Message if RPC check failed

## Migration from Process-Managed Zallet

If you previously used the pool's built-in Zallet process management:

1. Remove or ignore the `[zallet]` section from `pool.toml` (no longer used)
2. Start Zallet independently
3. Ensure Zallet RPC is reachable at `payout.wallet_rpc_url` (e.g. `http://127.0.0.1:28232`)
