use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::server::Server;

const RELAY_LIST_URL: &str = "https://api.mullvad.net/public/relays/wireguard/v1/";
const REGISTER_KEY_URL: &str = "https://api.mullvad.net/wg";

#[derive(Debug, Deserialize)]
struct ApiRelay {
    hostname: String,
    public_key: String,
    ipv4_addr_in: String,
}

#[derive(Debug, Deserialize)]
struct ApiCity {
    name: String,
    relays: Vec<ApiRelay>,
}

#[derive(Debug, Deserialize)]
struct ApiCountry {
    name: String,
    cities: Vec<ApiCity>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    countries: Vec<ApiCountry>,
}

/// Fetch the list of WireGuard servers from Mullvad API
pub async fn fetch_servers() -> Result<Vec<Server>> {
    let client = reqwest::Client::new();
    let response: ApiResponse = client
        .get(RELAY_LIST_URL)
        .send()
        .await?
        .json()
        .await?;

    let mut servers = Vec::new();

    for country in response.countries {
        for city in country.cities {
            for relay in city.relays {
                // Extract code from hostname (e.g., "se-mma-wg-001-wireguard" -> "se-mma-wg-001")
                let code = relay
                    .hostname
                    .strip_suffix("-wireguard")
                    .unwrap_or(&relay.hostname)
                    .to_string();

                servers.push(Server {
                    code,
                    hostname: relay.hostname,
                    public_key: relay.public_key,
                    ipv4_addr: relay.ipv4_addr_in,
                    port: 51820,
                    country: country.name.clone(),
                    city: city.name.clone(),
                });
            }
        }
    }

    Ok(servers)
}

/// Register a WireGuard public key with Mullvad account
/// Returns the assigned IP addresses on success
pub async fn register_public_key(account: &str, public_key: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(REGISTER_KEY_URL)
        .form(&[("account", account), ("pubkey", public_key)])
        .send()
        .await?;

    let text = response.text().await?;

    // Mullvad returns IP addresses on success, or an error message
    // Valid response format: "10.x.x.x/32,fc00:bbbb:bbbb:bb01::x:x/128"
    if text.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '/' || c == '.' || c == ',') {
        Ok(text)
    } else {
        Err(anyhow!("Mullvad API error: {}", text))
    }
}
