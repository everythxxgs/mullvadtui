use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::server::Server;

const WIREGUARD_DIR: &str = "/etc/wireguard";
const DNS_SERVER: &str = "10.64.0.1";

/// Get the path to a WireGuard config file for a server code
pub fn config_path(code: &str) -> PathBuf {
    Path::new(WIREGUARD_DIR).join(format!("{}.conf", code))
}

/// Check if a config file exists for the given server code
pub fn config_exists(code: &str) -> bool {
    config_path(code).exists()
}

/// List all existing Mullvad config files
pub fn list_configs() -> Result<Vec<String>> {
    let dir = Path::new(WIREGUARD_DIR);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut configs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext == "conf" {
                if let Some(name) = path.file_stem() {
                    let name = name.to_string_lossy().to_string();
                    // Only include Mullvad-style configs (e.g., se-mma-wg-001)
                    if name.contains("-wg-") {
                        configs.push(name);
                    }
                }
            }
        }
    }
    configs.sort();
    Ok(configs)
}

/// Extract private key from an existing config file
pub fn extract_private_key(code: &str) -> Result<Option<String>> {
    let path = config_path(code);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    for line in content.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("privatekey") {
            if let Some(pos) = line.find('=') {
                let key = line[pos + 1..].trim().to_string();
                if key.len() == 44 && key.ends_with('=') {
                    return Ok(Some(key));
                }
            }
        }
    }

    Ok(None)
}

/// Find any existing private key from Mullvad configs
pub fn find_existing_private_key() -> Result<Option<String>> {
    for code in list_configs()? {
        if let Some(key) = extract_private_key(&code)? {
            return Ok(Some(key));
        }
    }
    Ok(None)
}

/// Generate a WireGuard config file for a server
pub fn generate_config(
    server: &Server,
    private_key: &str,
    address: &str,
) -> Result<()> {
    let content = format!(
        "[Interface]\n\
         PrivateKey = {}\n\
         Address = {}\n\
         DNS = {}\n\
         \n\
         [Peer]\n\
         PublicKey = {}\n\
         Endpoint = {}\n\
         AllowedIPs = 0.0.0.0/0, ::/0\n",
        private_key,
        address,
        DNS_SERVER,
        server.public_key,
        server.endpoint()
    );

    let path = config_path(&server.code);
    let dir = path.parent().unwrap();

    // Ensure /etc/wireguard exists
    fs::create_dir_all(dir).context("Failed to create /etc/wireguard")?;

    // Write with restrictive permissions (0600)
    let tmp_path = path.with_extension("conf.tmp");
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_path)
        .and_then(|_| fs::write(&tmp_path, &content))
        .context("Failed to write config file")?;

    fs::rename(&tmp_path, &path).context("Failed to move config file")?;

    Ok(())
}

/// Delete a config file
pub fn delete_config(code: &str) -> Result<()> {
    let path = config_path(code);
    if path.exists() {
        fs::remove_file(&path).context("Failed to delete config file")?;
    }
    Ok(())
}

/// Generate configs for all servers
pub fn generate_all_configs(
    servers: &[Server],
    private_key: &str,
    address: &str,
) -> Result<usize> {
    let mut count = 0;
    for server in servers {
        generate_config(server, private_key, address)?;
        count += 1;
    }
    Ok(count)
}
