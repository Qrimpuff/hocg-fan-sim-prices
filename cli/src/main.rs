use std::{
    collections::HashMap,
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

    let mut all_prices_history: HashMap<PathBuf, (Option<PricesHistoryDatabase>, Vec<String>)> =
        HashMap::new();

    // load historical prices
    for (card_number, card) in &all_cards {
        let set = card_number.split('-').next().unwrap_or_default();
        let history_file = if card.card_type == CardType::Cheer {
            prices_path.join(format!("hY/{set}_history.json"))
        } else {
            prices_path.join(format!("{set}/{card_number}_history.json"))
        };

        if let Some(history) = all_prices_history.get_mut(&history_file) {
            history.1.push(card_number.clone());
            continue;
        }

        if !args.clean
            && let Ok(s) = fs::read_to_string(&history_file)
        {
            let history: PricesHistoryDatabase = serde_json::from_str(&s).unwrap();
            all_prices_history.insert(
                history_file.clone(),
                (Some(history), vec![card_number.clone()]),
            );
        } else {
            all_prices_history.insert(history_file.clone(), (None, vec![card_number.clone()]));
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
    for (history, card_numbers) in all_prices_history.values_mut() {
        for card_number in card_numbers {
            let card = all_cards.get(card_number).expect("is set above");

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
                let history = history.get_or_insert_default();
                for service_id in service_ids {
                    let Some(current_price) = all_prices.get(&service_id) else {
                        continue;
                    };
                    history
                        .entry(service_id)
                        .and_modify(|e| {
                            if e.last() != Some(current_price) {
                                e.push(*current_price);
                                e.sort_by_key(|(t, _)| *t);
                            }
                        })
                        .or_insert_with(|| vec![*current_price]);
                }
            }
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

    // save historical prices
    for (history_file, (history, _)) in all_prices_history {
        if let Some(history) = history {
            if let Some(parent) = history_file.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let mut json = vec![];
            let formatter = PrettyCompactFormatter::new();
            let mut ser = Serializer::with_formatter(&mut json, formatter);
            history.serialize(&mut ser).unwrap();
            fs::write(history_file, json).unwrap();
        }
    }

    println!("done");
}
