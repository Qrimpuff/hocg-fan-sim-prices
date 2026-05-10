# hocg-fan-sim-prices

A Rust workspace for collecting Hololive Official Card Game prices.

## Requirements

- Rust toolchain with edition 2024 support
- An `assets` directory containing `hocg_cards.json`

The card database usually comes from the companion [hocg-fan-sim-assets](https://github.com/Qrimpuff/hocg-fan-sim-assets) project.

## Optional env var

- `SCRAPERAPI_API_KEY` - optional ScraperAPI access for Yuyu-tei requests

## Run

```bash
cargo run -- [OPTIONS]
```

Options:

```text
-c, --clean
        Do not read existing price files before updating

    --assets-path <ASSETS_PATH>
        Folder containing card metadata
        Default: assets

    --prices-path <PRICES_PATH>
        Folder containing latest-price and history databases
        Default: prices

    --yuyutei
        Update prices from yuyu-tei.jp

    --tcgplayer
        Update prices from TCGplayer

-h, --help
        Print help

-V, --version
        Print version
```

## Examples

```bash
cargo run --release -- --yuyutei --tcgplayer
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
