use anyhow::{Context, Result};
use std::process::Command;

const MULLVAD_DNS: &str = "10.64.0.1";

/// Connection status
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connected(String), // Connected to server code
    Disconnected,
}

/// Check if a server is enabled for auto-start
pub fn is_enabled(code: &str) -> bool {
    let output = Command::new("systemctl")
        .args(["is-enabled", &format!("wg-quick@{}", code)])
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Get the currently enabled server (if any)
pub fn get_enabled_server() -> Option<String> {
    // List all wg-quick services and find enabled ones
    let output = Command::new("systemctl")
        .args(["list-unit-files", "wg-quick@*.service", "--no-legend"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "enabled" {
            // Extract server code from "wg-quick@se-mma-wg-001.service"
            if let Some(service) = parts[0].strip_prefix("wg-quick@") {
                if let Some(code) = service.strip_suffix(".service") {
                    if code.contains("-wg-") {
                        return Some(code.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Enable a server for auto-start on boot
pub fn enable_autostart(code: &str) -> Result<()> {
    // First disable any currently enabled Mullvad server
    if let Some(current) = get_enabled_server() {
        if current != code {
            let _ = Command::new("systemctl")
                .args(["disable", &format!("wg-quick@{}", current)])
                .output();
        }
    }

    let output = Command::new("systemctl")
        .args(["enable", &format!("wg-quick@{}", code)])
        .output()
        .context("Failed to run systemctl enable")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to enable service: {}", stderr);
    }

    Ok(())
}

/// Disable auto-start for a server
pub fn disable_autostart(code: &str) -> Result<()> {
    let output = Command::new("systemctl")
        .args(["disable", &format!("wg-quick@{}", code)])
        .output()
        .context("Failed to run systemctl disable")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to disable service: {}", stderr);
    }

    Ok(())
}

/// Connect to a WireGuard server using wg-quick
pub fn connect(code: &str) -> Result<()> {
    // Check if config exists
    let config_path = format!("/etc/wireguard/{}.conf", code);
    if !std::path::Path::new(&config_path).exists() {
        anyhow::bail!("Config file not found: {}. Press 'i' to setup.", config_path);
    }

    // Try to connect
    let output = try_wg_quick_up(code)?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // Check for resolvconf signature mismatch - fix it and retry
        if combined.contains("signature mismatch") {
            // Run resolvconf -u to fix
            let _ = Command::new("resolvconf").arg("-u").output();

            // Retry connection
            let retry_output = try_wg_quick_up(code)?;
            if retry_output.status.success() {
                // Configure DNS leak prevention
                configure_dns_leak_prevention(code);
                return Ok(());
            }

            // Still failed, get new error
            let retry_stdout = String::from_utf8_lossy(&retry_output.stdout);
            let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
            let retry_combined = format!("{}{}", retry_stdout, retry_stderr);
            anyhow::bail!("wg-quick up failed after resolvconf fix:\n{}", retry_combined.trim());
        }

        // Check for common errors - be specific about module loading failures
        if combined.contains("RTNETLINK answers: Operation not supported") {
            anyhow::bail!(
                "WireGuard module not loaded. Run: sudo modprobe wireguard"
            );
        }

        // Check if interface already exists
        if combined.contains("already exists") {
            anyhow::bail!("Interface already exists. Try disconnecting first (press 'd')");
        }

        anyhow::bail!("wg-quick up failed:\n{}", combined.trim());
    }

    // Configure DNS leak prevention
    configure_dns_leak_prevention(code);

    Ok(())
}

/// Configure DNS to prevent leaks
fn configure_dns_leak_prevention(interface: &str) {
    // Set DNS for the WireGuard interface
    let _ = Command::new("resolvectl")
        .args(["dns", interface, MULLVAD_DNS])
        .output();

    // Set this interface as the default route for DNS (~. means all domains)
    let _ = Command::new("resolvectl")
        .args(["domain", interface, "~."])
        .output();

    // Flush DNS cache
    let _ = Command::new("resolvectl")
        .arg("flush-caches")
        .output();

    // Block DNS on other interfaces with iptables (IPv4)
    let _ = Command::new("iptables")
        .args(["-I", "OUTPUT", "!", "-o", interface, "-p", "udp", "--dport", "53", "-j", "REJECT"])
        .output();
    let _ = Command::new("iptables")
        .args(["-I", "OUTPUT", "!", "-o", interface, "-p", "tcp", "--dport", "53", "-j", "REJECT"])
        .output();

    // Block DNS on other interfaces with iptables (IPv6)
    let _ = Command::new("ip6tables")
        .args(["-I", "OUTPUT", "!", "-o", interface, "-p", "udp", "--dport", "53", "-j", "REJECT"])
        .output();
    let _ = Command::new("ip6tables")
        .args(["-I", "OUTPUT", "!", "-o", interface, "-p", "tcp", "--dport", "53", "-j", "REJECT"])
        .output();
}

fn try_wg_quick_up(code: &str) -> Result<std::process::Output> {
    Command::new("wg-quick")
        .args(["up", code])
        .output()
        .context("Failed to execute wg-quick")
}

/// Disconnect from a WireGuard server using wg-quick
pub fn disconnect(code: &str) -> Result<()> {
    // Clean up DNS leak prevention rules first
    cleanup_dns_leak_prevention(code);

    let output = Command::new("wg-quick")
        .args(["down", code])
        .output()
        .context("Failed to execute wg-quick")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("wg-quick down failed: {}", stderr);
    }

    // Flush DNS cache after disconnect
    let _ = Command::new("resolvectl").arg("flush-caches").output();

    Ok(())
}

/// Clean up DNS leak prevention iptables rules
fn cleanup_dns_leak_prevention(interface: &str) {
    // Remove iptables rules (IPv4)
    let _ = Command::new("iptables")
        .args(["-D", "OUTPUT", "!", "-o", interface, "-p", "udp", "--dport", "53", "-j", "REJECT"])
        .output();
    let _ = Command::new("iptables")
        .args(["-D", "OUTPUT", "!", "-o", interface, "-p", "tcp", "--dport", "53", "-j", "REJECT"])
        .output();

    // Remove iptables rules (IPv6)
    let _ = Command::new("ip6tables")
        .args(["-D", "OUTPUT", "!", "-o", interface, "-p", "udp", "--dport", "53", "-j", "REJECT"])
        .output();
    let _ = Command::new("ip6tables")
        .args(["-D", "OUTPUT", "!", "-o", interface, "-p", "tcp", "--dport", "53", "-j", "REJECT"])
        .output();
}

/// Get current connection status by checking active interfaces
pub fn get_status() -> ConnectionStatus {
    // Try to get active WireGuard interfaces
    let output = Command::new("wg").arg("show").output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse the interface name from wg show output
            // Format: "interface: se-mma-wg-001"
            for line in stdout.lines() {
                if line.starts_with("interface:") {
                    let interface = line
                        .strip_prefix("interface:")
                        .map(|s| s.trim())
                        .unwrap_or("");
                    if !interface.is_empty() && interface.contains("-wg-") {
                        return ConnectionStatus::Connected(interface.to_string());
                    }
                }
            }
            ConnectionStatus::Disconnected
        }
        _ => ConnectionStatus::Disconnected,
    }
}

/// Generate a new WireGuard private key
pub fn generate_private_key() -> Result<String> {
    let output = Command::new("wg")
        .arg("genkey")
        .output()
        .context("Failed to execute wg genkey")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("wg genkey failed: {}", stderr);
    }

    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(key)
}

/// Get the public key from a private key
pub fn get_public_key(private_key: &str) -> Result<String> {
    // wg pubkey reads from stdin, so we need to pipe the private key
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn wg pubkey")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(private_key.as_bytes())
            .context("Failed to write to wg pubkey stdin")?;
    }

    let output = child.wait_with_output().context("Failed to wait for wg pubkey")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("wg pubkey failed: {}", stderr);
    }

    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(key)
}
