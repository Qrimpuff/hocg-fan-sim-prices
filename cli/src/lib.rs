pub mod price_check;

use std::{sync::OnceLock, time::Duration};

use reqwest::blocking::{Client, ClientBuilder};

pub const DEBUG: bool = false;

const HTTP_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:144.0) Gecko/20100101 Firefox/144.0";

fn http_client_base() -> ClientBuilder {
    ClientBuilder::new()
        .user_agent(HTTP_USER_AGENT)
        .cookie_store(true)
        .timeout(Duration::from_secs(70))
}

fn http_client() -> &'static Client {
    static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
    HTTP_CLIENT.get_or_init(|| http_client_base().build().unwrap())
}
