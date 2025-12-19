use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub code: String,
    pub hostname: String,
    pub public_key: String,
    pub ipv4_addr: String,
    pub port: u16,
    pub country: String,
    pub city: String,
}

impl Server {
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.ipv4_addr, self.port)
    }

    pub fn location(&self) -> String {
        format!("{}, {}", self.city, self.country)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerCache {
    pub servers: Vec<Server>,
    pub timestamp: u64,
}

/// Grouped servers by Country -> City -> Vec<Server>
pub type ServerTree = BTreeMap<String, BTreeMap<String, Vec<Server>>>;

/// Group servers into a hierarchical tree structure
pub fn group_servers(servers: &[Server]) -> ServerTree {
    let mut tree: ServerTree = BTreeMap::new();

    for server in servers {
        tree.entry(server.country.clone())
            .or_default()
            .entry(server.city.clone())
            .or_default()
            .push(server.clone());
    }

    // Sort servers within each city by code
    for country in tree.values_mut() {
        for servers in country.values_mut() {
            servers.sort_by(|a, b| a.code.cmp(&b.code));
        }
    }

    tree
}

/// Get list of countries from server tree
pub fn get_countries(tree: &ServerTree) -> Vec<String> {
    tree.keys().cloned().collect()
}

/// Get list of cities for a country
pub fn get_cities(tree: &ServerTree, country: &str) -> Vec<String> {
    tree.get(country)
        .map(|cities| cities.keys().cloned().collect())
        .unwrap_or_default()
}

/// Get servers for a specific city
pub fn get_servers_in_city(tree: &ServerTree, country: &str, city: &str) -> Vec<Server> {
    tree.get(country)
        .and_then(|cities| cities.get(city))
        .cloned()
        .unwrap_or_default()
}
