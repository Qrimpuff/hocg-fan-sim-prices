use std::{
    collections::{HashMap, HashSet},
    fs::{self},
    path::{Path, PathBuf},
};

use clap::Parser;
use hocg_fan_sim_assets_model::{CardType, CardsDatabase};
use hocg_fan_sim_prices_cli::price_check::{tcgplayer, yuyutei};
use hocg_fan_sim_prices_model::{PricesDatabase, PricesHistoryDatabase, ServiceId};
use json_pretty_compact::PrettyCompactFormatter;
use serde::Serialize;
use serde_json::Serializer;
use walkdir::WalkDir;

/// Scrap hOCG price from Yuyu-tei and TCGplayer
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Don't read existing file
    #[arg(short = 'c', long)]
    clean: bool,

    /// The folder that contains the assets i.e. card info, images, proxies
    #[arg(long, default_value = "assets")]
    assets_path: PathBuf,

    /// The folder that contains the prices and history databases
    #[arg(long, default_value = "prices")]
    prices_path: PathBuf,

    /// Update the yuyu-tei.jp prices
    #[arg(long)]
    yuyutei: bool,

    /// Update the TCGplayer prices
    #[arg(long)]
    tcgplayer: bool,
}

fn main() {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let assets_path = args.assets_path.as_path();
    let card_mapping_file = assets_path.join("hocg_cards.json");

    let all_cards: CardsDatabase =
        serde_json::from_str(&fs::read_to_string(&card_mapping_file).unwrap()).unwrap();

    let prices_path = args.prices_path.as_path();
    let prices_mapping_file = prices_path.join("hocg_prices.json");

    let mut all_prices: PricesDatabase = PricesDatabase::new();

    // load file
    if !args.clean
        && let Ok(s) = fs::read_to_string(&prices_mapping_file)
    {
        all_prices = serde_json::from_str(&s).unwrap();
    }

    let mut all_prices_history: PricesHistoryDatabase = PricesHistoryDatabase::new();

    // load historical prices
    if !args.clean {
        for entry in WalkDir::new(prices_path)
            .contents_first(true)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter(|e| !e.path().components().any(|c| c.as_os_str() == ".git"))
            .filter(|e| e.file_name().to_string_lossy().ends_with("_history.json"))
        {
            let history: PricesHistoryDatabase =
                serde_json::from_str(&fs::read_to_string(entry.path()).unwrap()).unwrap();
            all_prices_history.extend(history);
        }
    }

    // update yuyutei prices
    if args.yuyutei {
        yuyutei(&mut all_prices);
    }

    // update tcgplayer prices
    if args.tcgplayer {
        tcgplayer(&mut all_prices);
    }

    // update historical prices
    for (service, price) in &all_prices {
        let history = all_prices_history.entry(service.clone()).or_default();
        if history.last() != Some(price) {
            history.push(*price);
            history.sort();
        }
    }

    // save file
    if let Some(parent) = Path::new(&prices_mapping_file).parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut json = vec![];
    let formatter = PrettyCompactFormatter::new();
    let mut ser = Serializer::with_formatter(&mut json, formatter);
    all_prices.serialize(&mut ser).unwrap();
    fs::write(&prices_mapping_file, json).unwrap();

    // split history by card number
    let mut card_prices_history: HashMap<PathBuf, PricesHistoryDatabase> = HashMap::new();
    for (card_number, card) in all_cards {
        let set = card_number.split('-').next().unwrap_or_default();
        let history_file = if card.card_type == CardType::Cheer {
            prices_path.join(format!("hY/{set}_history.json"))
        } else {
            prices_path.join(format!("{set}/{card_number}_history.json"))
        };

        let service_ids: Vec<_> = card
            .illustrations
            .iter()
            .filter_map(|i| i.yuyutei_sell_url.as_ref())
            .map(|url| ServiceId::from_yuyutei(url.clone()))
            .chain(
                card.illustrations
                    .iter()
                    .filter_map(|i| i.tcgplayer_product_id)
                    .map(ServiceId::from_tcgplayer),
            )
            .collect();

        if !service_ids.is_empty() {
            let history = card_prices_history.entry(history_file).or_default();
            for service_id in service_ids {
                if let Some(price_history) = all_prices_history.remove(&service_id) {
                    history.insert(service_id, price_history);
                }
            }
        }
    }

    // save historical prices
    for (history_file, history) in &card_prices_history {
        if let Some(parent) = history_file.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut json = vec![];
        let formatter = PrettyCompactFormatter::new();
        let mut ser = Serializer::with_formatter(&mut json, formatter);
        history.serialize(&mut ser).unwrap();
        fs::write(history_file, json).unwrap();
    }

    // save remainder in a "other_history.json"
    let other_history_file = prices_path.join("other_history.json");
    if !all_prices_history.is_empty() {
        if let Some(parent) = Path::new(&other_history_file).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut json = vec![];
        let formatter = PrettyCompactFormatter::new();
        let mut ser = Serializer::with_formatter(&mut json, formatter);
        all_prices_history.serialize(&mut ser).unwrap();
        fs::write(&other_history_file, json).unwrap();
    }

    // delete old history files
    let mut required_paths = HashSet::from([prices_mapping_file.to_owned()]);
    required_paths.extend(card_prices_history.keys().cloned());
    if !all_prices_history.is_empty() {
        required_paths.insert(other_history_file);
    }
    for entry in WalkDir::new(prices_path)
        .contents_first(true)
        .into_iter()
        .flatten()
        .filter(|e| !e.path().components().any(|c| c.as_os_str() == ".git"))
    {
        // keep referenced files
        if required_paths.contains(entry.path()) {
            continue;
        }

        if entry.file_type().is_file() {
            // remove file
            println!(
                "Removing file: {}",
                entry.path().strip_prefix(prices_path).unwrap().display()
            );
            fs::remove_file(entry.path()).unwrap();
        } else if entry.file_type().is_dir() {
            // remove folder, if it's empty
            if entry.path().read_dir().unwrap().next().is_none() {
                println!(
                    "Removing empty folder: {}",
                    entry.path().strip_prefix(prices_path).unwrap().display()
                );
                fs::remove_dir(entry.path()).unwrap();
            }
        }
    }

    println!("done");
}
