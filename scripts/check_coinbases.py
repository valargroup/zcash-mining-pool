#!/usr/bin/env python3
"""Fetch coinbase info for the last N blocks from zebrad RPC."""
import json
import os
import sys
import urllib.request

RPC_URL = os.environ.get("ZEBRAD_RPC_URL", "http://127.0.0.1:18232")

def rpc(method, params):
    body = json.dumps({"jsonrpc": "1.0", "id": "1", "method": method, "params": params}).encode()
    req = urllib.request.Request(RPC_URL, data=body, headers={"Content-Type": "text/plain"})
    with urllib.request.urlopen(req, timeout=30) as r:
        out = json.loads(r.read().decode())
    if out.get("error"):
        raise RuntimeError(out["error"])
    return out.get("result")

def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
    height = rpc("getblockcount", [])
    start = max(0, height - n + 1)
    print(f"Block height: {start} .. {height} ({height - start + 1} blocks)\n")

    by_address = {}
    rows = []

    for h in range(start, height + 1):
        blk = rpc("getblock", [str(h), 1])
        if not blk or "tx" not in blk or not blk["tx"]:
            rows.append((h, None, "no tx", 0, "", "", False))
            continue
        txid = blk["tx"][0]
        tx = rpc("getrawtransaction", [txid, 1])
        if not tx or "vout" not in tx:
            rows.append((h, txid, "no vout", 0, "", "", False))
            continue
        # coinbase: vout[0] = miner reward (1.25 ZEC), vout[1] = funding stream (0.125 ZEC)
        miner_vout = tx["vout"][0] if tx["vout"] else {}
        spk = miner_vout.get("scriptPubKey") or {}
        addrs = spk.get("addresses") or []
        addr = addrs[0] if addrs else "unknown"
        amt = miner_vout.get("valueZat", 0) or miner_vout.get("valueSat", 0) or 0
        # Coinbase scriptSig (hex) - pool tag "Legends" may appear here
        vin = (tx.get("vin") or [])
        coinbase_hex = vin[0].get("coinbase", "") if vin else ""
        coinbase_text = bytes.fromhex(coinbase_hex).decode("utf-8", errors="replace") if coinbase_hex else ""
        # "Legends" in ASCII hex is 4c6567656e6473
        has_legends = "Legends" in coinbase_text or "4c6567656e6473" in coinbase_hex.lower()
        by_address[addr] = by_address.get(addr, 0) + 1
        rows.append((h, txid[:16] + "...", addr, amt, coinbase_hex, coinbase_text.strip(), has_legends))

    # Table
    print(f"{'Height':<10} {'Miner address':<45} {'Reward':<8} {'Legends':<8} {'Coinbase script (text)'}")
    print("-" * 95)
    for row in rows:
        h, txid, addr, zat, cb_hex, cb_text, has_legends = row
        zec = zat / 1e8 if zat else 0
        legends = "YES" if has_legends else ""
        # Show readable part of coinbase (after height bytes); truncate long
        disp = (cb_text or "")[:50]
        print(f"{h:<10} {addr:<45} {zec:.4f}   {legends:<8} {disp}")

    legends_blocks = [r[0] for r in rows if r[6]]
    if legends_blocks:
        print(f"\n--- Blocks with 'Legends' in coinbase: {len(legends_blocks)} ---")
        print(f"  Heights: {legends_blocks}")
    else:
        print("\n--- No blocks had 'Legends' in coinbase (none from this pool in this range) ---")

    print("\n--- By miner address (block count) ---")
    for addr, count in sorted(by_address.items(), key=lambda x: -x[1]):
        print(f"  {count:>4} blocks  {addr}")

    # Coinbase hex and ASCII for each block
    print("\n" + "=" * 80)
    print("COINBASE HEX AND ASCII (last N blocks)")
    print("=" * 80)
    for row in rows:
        h, _, addr, _, cb_hex, _, has_legends = row
        if not cb_hex:
            print(f"\nBlock {h}: (no coinbase)")
            continue
        raw = bytes.fromhex(cb_hex)
        ascii_disp = "".join(chr(b) if 32 <= b < 127 else "." for b in raw)
        print(f"\nBlock {h}  Miner: {addr}  Legends: {'YES' if has_legends else 'no'}")
        print(f"  Hex:   {cb_hex}")
        print(f"  ASCII: {ascii_disp}")

if __name__ == "__main__":
    main()
