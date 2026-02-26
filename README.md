# rusty_ticker

A live market watcher built in Rust for the terminal.

## Features

- Real-time refreshing ticker table (like a compact `top`-style dashboard).
- Pulls 24h market stats from Binance's public REST API.
- Tracks one or many symbols.
- Keyboard controls (`q` or `Ctrl+C` to quit).

## Usage

```bash
cargo run -- --symbols BTCUSDT,ETHUSDT,SOLUSDT --interval 2
```

Options:

- `--symbols` / `-s`: Comma-separated symbols to track.
- `--interval` / `-i`: Refresh interval in seconds.

## Notes

- Data source: `https://api.binance.com/api/v3/ticker/24hr`
- No API key required for the endpoint used in this starter tool.
