use std::{
    collections::BTreeMap,
    fmt::Display,
    ops::{Add, Sub},
    str::FromStr,
};

use icu::{
    decimal::{DecimalFormatter, input::Decimal},
    locale::locale,
};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

pub type PricesDatabase = BTreeMap<ServiceId, (Timestamp, Price)>;
pub type PricesHistoryDatabase = BTreeMap<ServiceId, Vec<(Timestamp, Price)>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ServiceId {
    Yuyutei(String),
    TcgPlayer(String),
}

impl From<String> for ServiceId {
    fn from(s: String) -> Self {
        if s.contains("tcgplayer.com") {
            ServiceId::TcgPlayer(s)
        } else if s.contains("yuyu-tei.jp") {
            ServiceId::Yuyutei(s)
        } else {
            panic!("Unknown url: {s}");
        }
    }
}

impl From<ServiceId> for String {
    fn from(service_id: ServiceId) -> Self {
        match service_id {
            ServiceId::Yuyutei(url) => url,
            ServiceId::TcgPlayer(url) => url,
        }
    }
}

impl ServiceId {
    pub fn new_yuyutei(url: String) -> Self {
        ServiceId::Yuyutei(url)
    }

    pub fn new_tcgplayer(product_id: u32) -> Self {
        ServiceId::TcgPlayer(format!("https://www.tcgplayer.com/product/{product_id}"))
    }

    pub fn url(&self) -> &String {
        match &self {
            ServiceId::Yuyutei(url) => url,
            ServiceId::TcgPlayer(url) => url,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Price {
    #[serde(rename = "y")]
    Yen(u32),
    #[serde(rename = "d")]
    Dollar(u32),
}

impl Price {
    pub fn new_yen(price: u32) -> Self {
        Price::Yen(price)
    }

    pub fn new_dollar(price: f64) -> Self {
        Price::Dollar((price * 100.0).round() as u32)
    }

    pub fn as_float(&self) -> f64 {
        match self {
            Price::Yen(p) => *p as f64,
            Price::Dollar(p) => *p as f64 / 100.0,
        }
    }
}

impl Add for Price {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Price::Yen(a), Price::Yen(b)) => Price::Yen(a + b),
            (Price::Dollar(a), Price::Dollar(b)) => Price::Dollar(a + b),
            _ => panic!("cannot add different price types"),
        }
    }
}

impl Sub for Price {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Price::Yen(a), Price::Yen(b)) => Price::Yen(a - b),
            (Price::Dollar(a), Price::Dollar(b)) => Price::Dollar(a - b),
            _ => panic!("cannot subtract different price types"),
        }
    }
}

impl Display for Price {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Price::Yen(p) => {
                let f = DecimalFormatter::try_new(locale!("ja-JP").into(), Default::default())
                    .expect("locale should be present");
                let p = Decimal::from_str(format!("{p}").as_str()).unwrap();
                let p = f.format(&p);
                write!(fmt, "¥{}", p)
            }
            Price::Dollar(p) => {
                let p = *p as f64 / 100.0;
                let f = DecimalFormatter::try_new(locale!("en-US").into(), Default::default())
                    .expect("locale should be present");
                let p = Decimal::from_str(format!("{p:.2}").as_str()).unwrap();
                let p = f.format(&p);
                write!(fmt, "${}", p)
            }
        }
    }
}
