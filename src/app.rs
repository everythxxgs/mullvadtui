use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api;
use crate::config;
use crate::server::{group_servers, get_cities, get_countries, get_servers_in_city, Server, ServerCache, ServerTree};
use crate::wireguard::{self, ConnectionStatus};

/// Current view/screen in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Countries,
    Cities,
    Servers,
    Setup,
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    AccountInput,
}

/// Application state
pub struct App {
    pub view: View,
    pub input_mode: InputMode,
    pub input_buffer: String,

    // Server data
    pub servers: Vec<Server>,
    pub server_tree: ServerTree,

    // Navigation state
    pub countries: Vec<String>,
    pub cities: Vec<String>,
    pub city_servers: Vec<Server>,

    pub selected_country_idx: usize,
    pub selected_city_idx: usize,
    pub selected_server_idx: usize,

    pub selected_country: Option<String>,
    pub selected_city: Option<String>,

    // Connection status
    pub connection_status: ConnectionStatus,

    // Autostart server (enabled for systemd)
    pub autostart_server: Option<String>,

    // Messages
    pub message: Option<String>,
    pub error: Option<String>,

    // Setup state
    pub private_key: Option<String>,
    pub address: Option<String>,

    // Should quit
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            view: View::Countries,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),

            servers: Vec::new(),
            server_tree: ServerTree::new(),

            countries: Vec::new(),
            cities: Vec::new(),
            city_servers: Vec::new(),

            selected_country_idx: 0,
            selected_city_idx: 0,
            selected_server_idx: 0,

            selected_country: None,
            selected_city: None,

            connection_status: ConnectionStatus::Disconnected,

            autostart_server: None,

            message: None,
            error: None,

            private_key: None,
            address: None,

            should_quit: false,
        }
    }

    /// Initialize the app - load cache and check status
    pub async fn init(&mut self) -> Result<()> {
        // Load cached servers
        if let Ok(Some(cache)) = load_cache() {
            self.servers = cache.servers;
            self.server_tree = group_servers(&self.servers);
            self.countries = get_countries(&self.server_tree);
        }

        // Check connection status
        self.connection_status = wireguard::get_status();

        // Check which server is enabled for autostart
        self.autostart_server = wireguard::get_enabled_server();

        // Try to find existing private key
        self.private_key = config::find_existing_private_key()?;

        Ok(())
    }

    /// Refresh servers from API
    pub async fn refresh_servers(&mut self) -> Result<()> {
        self.message = Some("Fetching servers...".to_string());
        self.error = None;

        match api::fetch_servers().await {
            Ok(servers) => {
                self.servers = servers;
                self.server_tree = group_servers(&self.servers);
                self.countries = get_countries(&self.server_tree);

                // Reset navigation
                self.selected_country_idx = 0;
                self.selected_city_idx = 0;
                self.selected_server_idx = 0;

                // Save cache
                save_cache(&self.servers)?;

                self.message = Some(format!("Loaded {} servers", self.servers.len()));
            }
            Err(e) => {
                self.error = Some(format!("Failed to fetch servers: {}", e));
            }
        }

        Ok(())
    }

    /// Update connection status
    pub fn update_status(&mut self) {
        self.connection_status = wireguard::get_status();
    }

    /// Navigate to next item in current list
    pub fn next(&mut self) {
        match self.view {
            View::Countries => {
                if !self.countries.is_empty() {
                    self.selected_country_idx =
                        (self.selected_country_idx + 1) % self.countries.len();
                }
            }
            View::Cities => {
                if !self.cities.is_empty() {
                    self.selected_city_idx = (self.selected_city_idx + 1) % self.cities.len();
                }
            }
            View::Servers => {
                if !self.city_servers.is_empty() {
                    self.selected_server_idx =
                        (self.selected_server_idx + 1) % self.city_servers.len();
                }
            }
            View::Setup => {}
        }
    }

    /// Navigate to previous item in current list
    pub fn previous(&mut self) {
        match self.view {
            View::Countries => {
                if !self.countries.is_empty() {
                    self.selected_country_idx = if self.selected_country_idx == 0 {
                        self.countries.len() - 1
                    } else {
                        self.selected_country_idx - 1
                    };
                }
            }
            View::Cities => {
                if !self.cities.is_empty() {
                    self.selected_city_idx = if self.selected_city_idx == 0 {
                        self.cities.len() - 1
                    } else {
                        self.selected_city_idx - 1
                    };
                }
            }
            View::Servers => {
                if !self.city_servers.is_empty() {
                    self.selected_server_idx = if self.selected_server_idx == 0 {
                        self.city_servers.len() - 1
                    } else {
                        self.selected_server_idx - 1
                    };
                }
            }
            View::Setup => {}
        }
    }

    /// Select current item (enter)
    pub fn select(&mut self) {
        match self.view {
            View::Countries => {
                if let Some(country) = self.countries.get(self.selected_country_idx) {
                    self.selected_country = Some(country.clone());
                    self.cities = get_cities(&self.server_tree, country);
                    self.selected_city_idx = 0;
                    self.view = View::Cities;
                }
            }
            View::Cities => {
                if let Some(country) = &self.selected_country {
                    if let Some(city) = self.cities.get(self.selected_city_idx) {
                        self.selected_city = Some(city.clone());
                        self.city_servers = get_servers_in_city(&self.server_tree, country, city);
                        self.selected_server_idx = 0;
                        self.view = View::Servers;
                    }
                }
            }
            View::Servers => {
                // Connect to selected server
                if let Some(server) = self.city_servers.get(self.selected_server_idx) {
                    self.connect_to_server(&server.code.clone());
                }
            }
            View::Setup => {}
        }
    }

    /// Go back to previous view
    pub fn back(&mut self) {
        match self.view {
            View::Countries => {}
            View::Cities => {
                self.view = View::Countries;
                self.selected_country = None;
            }
            View::Servers => {
                self.view = View::Cities;
                self.selected_city = None;
            }
            View::Setup => {
                self.view = View::Countries;
                self.input_mode = InputMode::Normal;
            }
        }
    }

    /// Connect to a server
    pub fn connect_to_server(&mut self, code: &str) {
        // First disconnect if connected
        if let ConnectionStatus::Connected(current) = &self.connection_status {
            if let Err(e) = wireguard::disconnect(current) {
                self.error = Some(format!("Failed to disconnect: {}", e));
                return;
            }
        }

        // Check if config exists
        if !config::config_exists(code) {
            self.error = Some(format!("No config for {}. Press 'i' to initialize.", code));
            return;
        }

        // Connect
        match wireguard::connect(code) {
            Ok(()) => {
                self.connection_status = ConnectionStatus::Connected(code.to_string());
                self.message = Some(format!("Connected to {}", code));
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!("Failed to connect: {}", e));
            }
        }
    }

    /// Disconnect from current server
    pub fn disconnect(&mut self) {
        if let ConnectionStatus::Connected(code) = &self.connection_status.clone() {
            match wireguard::disconnect(code) {
                Ok(()) => {
                    self.connection_status = ConnectionStatus::Disconnected;
                    self.message = Some("Disconnected".to_string());
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(format!("Failed to disconnect: {}", e));
                }
            }
        }
    }

    /// Enter setup mode
    pub fn enter_setup(&mut self) {
        self.view = View::Setup;
        self.input_mode = InputMode::AccountInput;
        self.input_buffer.clear();
    }

    /// Handle setup submission (account number entered)
    pub async fn submit_setup(&mut self) -> Result<()> {
        let account = self.input_buffer.trim().to_string();
        if account.is_empty() {
            self.error = Some("Account number cannot be empty".to_string());
            return Ok(());
        }

        self.message = Some("Setting up...".to_string());
        self.error = None;

        // Get or generate private key
        let private_key = match &self.private_key {
            Some(key) => {
                self.message = Some("Using existing private key...".to_string());
                key.clone()
            }
            None => {
                self.message = Some("Generating new private key...".to_string());
                wireguard::generate_private_key()?
            }
        };

        // Get public key
        let public_key = wireguard::get_public_key(&private_key)?;

        // Register with Mullvad
        self.message = Some("Registering with Mullvad...".to_string());
        let address = api::register_public_key(&account, &public_key).await?;

        // Fetch servers if needed
        if self.servers.is_empty() {
            self.message = Some("Fetching servers...".to_string());
            self.servers = api::fetch_servers().await?;
            self.server_tree = group_servers(&self.servers);
            self.countries = get_countries(&self.server_tree);
            save_cache(&self.servers)?;
        }

        // Generate all configs
        self.message = Some("Generating config files...".to_string());
        let count = config::generate_all_configs(&self.servers, &private_key, &address)?;

        self.private_key = Some(private_key);
        self.address = Some(address);

        self.message = Some(format!(
            "Setup complete! Generated {} config files.",
            count
        ));
        self.input_mode = InputMode::Normal;
        self.view = View::Countries;

        Ok(())
    }

    /// Get current list length for display
    pub fn current_list_len(&self) -> usize {
        match self.view {
            View::Countries => self.countries.len(),
            View::Cities => self.cities.len(),
            View::Servers => self.city_servers.len(),
            View::Setup => 0,
        }
    }

    /// Get current selection index
    pub fn current_selection(&self) -> usize {
        match self.view {
            View::Countries => self.selected_country_idx,
            View::Cities => self.selected_city_idx,
            View::Servers => self.selected_server_idx,
            View::Setup => 0,
        }
    }

    /// Toggle autostart for the currently selected server
    pub fn toggle_autostart(&mut self) {
        if self.view != View::Servers {
            return;
        }

        if let Some(server) = self.city_servers.get(self.selected_server_idx) {
            let code = server.code.clone();

            // Check if this server is already enabled
            let is_currently_enabled = self.autostart_server.as_ref() == Some(&code);

            if is_currently_enabled {
                // Disable it
                match wireguard::disable_autostart(&code) {
                    Ok(()) => {
                        self.autostart_server = None;
                        self.message = Some(format!("Disabled autostart for {}", code));
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to disable autostart: {}", e));
                    }
                }
            } else {
                // Enable it (will disable any other)
                match wireguard::enable_autostart(&code) {
                    Ok(()) => {
                        self.autostart_server = Some(code.clone());
                        self.message = Some(format!("Enabled autostart for {}", code));
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to enable autostart: {}", e));
                    }
                }
            }
        }
    }
}

fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("mullvadtui")
        .join("servers.json")
}

fn load_cache() -> Result<Option<ServerCache>> {
    let path = cache_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    let cache: ServerCache = serde_json::from_str(&content)?;
    Ok(Some(cache))
}

fn save_cache(servers: &[Server]) -> Result<()> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let cache = ServerCache {
        servers: servers.to_vec(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    let content = serde_json::to_string_pretty(&cache)?;
    fs::write(&path, content)?;

    Ok(())
}
