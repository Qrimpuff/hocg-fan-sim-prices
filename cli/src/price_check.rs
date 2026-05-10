use std::{sync::Arc, time::Duration};

use hocg_fan_sim_prices_model::{Price, PricesDatabase, ServiceId};
use indexmap::IndexMap;
use jiff::Timestamp;
use parking_lot::{Condvar, Mutex, RwLock};
use rayon::iter::{ParallelBridge, ParallelIterator};
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::Value;

use crate::http_client;

pub fn yuyutei(all_prices: &mut PricesDatabase) {
    println!("Scraping Yuyutei prices...",);

    let scraperapi_key = std::env::var("SCRAPERAPI_API_KEY").ok();
    if scraperapi_key.is_some() {
        println!("using scraperapi.com");
    }

    // handle multiple pages (one page is 600 cards)
    // could be slow when there are multiple pages
    // Process pages in parallel
    let urls = Arc::new(RwLock::new(IndexMap::new()));
    let max_page = Arc::new((Mutex::new(0), Condvar::new()));
    let _ = (1..)
        .par_bridge()
        .map({
            let urls = urls.clone();
            let max_page = max_page.clone();
            move |page| {
                // wait for the max page to be set
                if page > 1 {
                    let mut max_page_lock = max_page.0.lock();
                    if *max_page_lock == 0 {
                        max_page
                            .1
                            .wait_for(&mut max_page_lock, Duration::from_secs(600));
                    } else {
                        max_page.1.notify_all();
                    }

                    if page > *max_page_lock {
                        return None;
                    }
                }

                let unblock_workers = || {
                    // Avoid a long wait on other workers if the first page fails
                    // or pagination metadata is missing.
                    if page == 1 {
                        let mut max_page_lock = max_page.0.lock();
                        if *max_page_lock == 0 {
                            *max_page_lock = 1;
                        }
                    }
                    max_page.1.notify_all();
                };

                let mut url = Url::parse("https://yuyu-tei.jp/sell/hocg/s/search").unwrap();
                url.query_pairs_mut()
                    .append_pair("search_word", "h") // every card number starts with "h"
                    .append_pair("page", page.to_string().as_str());
                let resp = match if let Some(scraperapi_key) = &scraperapi_key {
                    http_client()
                        .get("https://api.scraperapi.com/")
                        .query(&[
                            ("api_key", scraperapi_key.as_str()),
                            ("url", url.as_str()),
                            ("session_number", "123"),
                        ])
                        .send()
                } else {
                    http_client().get(url.clone()).send()
                } {
                    Ok(resp) => resp,
                    Err(err) => {
                        eprintln!("WARNING: Failed to fetch Yuyutei page {page}: {err}");
                        unblock_workers();
                        return Some(());
                    }
                };

                let content = match resp.text() {
                    Ok(content) => content,
                    Err(err) => {
                        eprintln!("WARNING: Failed to read Yuyutei page {page}: {err}");
                        unblock_workers();
                        return Some(());
                    }
                };
                // println!("{content}");

                let document = Html::parse_document(&content);
                let card_lists = Selector::parse("#card-list3").unwrap();
                let cards_select = Selector::parse(".card-product").unwrap();
                let url_select = Selector::parse("a").unwrap();
                let price_select = Selector::parse("strong").unwrap();
                let max_page_select =
                    Selector::parse(".pagination li:nth-last-child(2) a").unwrap();

                if let Some(max) = document.select(&max_page_select).next()
                    && let Ok(max_page_num) = max.text().collect::<String>().parse()
                {
                    *max_page.0.lock() = max_page_num;
                }

                // If page count is unavailable (or failed to parse), at least unblock workers.
                unblock_workers();

                // Extract price data
                for card_list in document.select(&card_lists) {
                    for card in card_list.select(&cards_select) {
                        let url = card.select(&url_select).next().unwrap().attr("href");
                        let price: String =
                            card.select(&price_select).next().unwrap().text().collect();
                        let price_yen = price
                            .replace(",", "")
                            .replace("円", "")
                            .trim()
                            .parse::<u32>()
                            .unwrap();
                        if let Some(url) = url {
                            // group them by url
                            urls.write().entry(url.to_owned()).or_insert(price_yen);
                        }
                    }
                }

                let max_page_num = *max_page.0.lock();
                println!("Page {page}/{max_page_num} done");
                (page < max_page_num).then_some(())
            }
        })
        .while_some()
        .max(); // Need this to drive the iterator

    let urls = Arc::try_unwrap(urls).unwrap().into_inner();
    println!("Found {} Yuyutei urls...", urls.len());
    // println!("BEFORE: {urls:#?}");

    // update prices
    let mut url_count = 0;
    let mut url_skipped = 0;
    for (url, price) in urls {
        let price = Price::new_yen(price);
        all_prices
            .entry(ServiceId::new_yuyutei(url.clone()))
            .and_modify(|p| {
                if p.1 != price {
                    p.0 = Timestamp::now();
                    p.1 = price.clone();
                    url_count += 1;
                } else {
                    url_skipped += 1;
                }
            })
            .or_insert_with(|| {
                url_count += 1;
                (Timestamp::now(), price)
            });
    }

    println!("{url_count} Yuyutei prices updated ({url_skipped} skipped)");
}

pub fn tcgplayer(all_prices: &mut PricesDatabase) {
    println!("Scraping TCGplayer prices...",);

    const HOCG_CATEGORY_ID: &str = "87";

    println!("Fetching TCGplayer groups...");
    let groups_url = format!("https://tcgcsv.com/tcgplayer/{HOCG_CATEGORY_ID}/groups");
    let resp = http_client().get(&groups_url).send().unwrap();

    let all_groups = resp.json::<Value>().unwrap();
    let all_groups = all_groups["results"].as_array().unwrap();
    println!("Found {} TCGplayer groups.", all_groups.len());

    let mut product_ids = IndexMap::new();
    for group in all_groups {
        let group_name = group["name"].as_str().unwrap_or("Unknown Group");
        let group_id = group["groupId"].as_u64().unwrap();
        println!("Processing group: {group_name}");

        // Fetch prices
        let prices_url =
            format!("https://tcgcsv.com/tcgplayer/{HOCG_CATEGORY_ID}/{group_id}/prices");
        let resp = http_client().get(&prices_url).send().unwrap();

        #[derive(serde::Deserialize, Debug)]
        #[serde(rename_all = "camelCase")]
        struct TcgPlayerPrice {
            product_id: u32,
            mid_price: Option<f64>,
            market_price: Option<f64>,
        }

        // Extract price data
        let prices = resp.json::<Value>().unwrap();
        let prices = prices["results"].as_array().unwrap();
        for price in prices {
            let price: TcgPlayerPrice = serde_json::from_value(price.clone()).unwrap();
            if price.mid_price.is_none() && price.market_price.is_none() {
                // skip products without prices
                continue;
            }

            product_ids
                .entry(price.product_id)
                .or_insert(price.market_price.unwrap_or_else(|| {
                    // if no market price, use mid price
                    price.mid_price.unwrap()
                }));
        }
    }

    println!("Found {} TCGplayer products...", product_ids.len());

    // update prices
    let mut product_id_count = 0;
    let mut product_ids_skipped = 0;
    for (product_id, price) in product_ids {
        let price = Price::new_dollar(price);
        all_prices
            .entry(ServiceId::new_tcgplayer(product_id))
            .and_modify(|p| {
                if p.1 != price {
                    p.0 = Timestamp::now();
                    p.1 = price.clone();
                    product_id_count += 1;
                } else {
                    product_ids_skipped += 1;
                }
            })
            .or_insert_with(|| {
                product_id_count += 1;
                (Timestamp::now(), price)
            });
    }

    println!("{product_id_count} TCGplayer prices updated ({product_ids_skipped} skipped)");
}
