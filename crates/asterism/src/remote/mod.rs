pub mod server;
pub mod tailscale;
pub mod token;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use crate::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    RemoteConfig, RemoteDevice, RemoteInterface, RemotePairing, RemoteServe, RemoteState,
    RemoteStatus,
};
use tokio::sync::{broadcast, Notify};

use crate::agent_settings::{write_atomic, SAVE_LOCK};
use crate::config::Config;
use crate::daemon::Daemon;
use crate::error::{Error, Result};
use crate::lock;
use crate::store::StoredRemoteDevice;

#[derive(Clone, Default)]
pub struct Listener {
    pub state: RemoteState,
    pub address: Option<SocketAddr>,
    pub error: Option<String>,
    pub public_url: Option<String>,
    pub serve_command: Option<String>,
}

impl Listener {
    pub fn failed(error: String) -> Self {
        Self {
            state: RemoteState::Error,
            error: Some(error),
            ..Self::default()
        }
    }
}

pub struct Hub {
    listener: Mutex<Listener>,
    /// Wakes the listener loop to rebind after a config change.
    pub(crate) changed: Notify,
    closing: broadcast::Sender<Option<i64>>,
    connections: Mutex<HashMap<i64, u32>>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            listener: Mutex::new(Listener::default()),
            changed: Notify::new(),
            closing: broadcast::channel(16).0,
            connections: Mutex::new(HashMap::new()),
        }
    }

    pub fn set(&self, listener: Listener) {
        *lock(&self.listener) = listener;
    }

    pub fn bound(&self) -> Option<SocketAddr> {
        lock(&self.listener).address
    }

    /// Behind serve only the HTTPS address reaches the listener, so pairing fails without it.
    fn https_base(&self, config: &RemoteConfig) -> Result<Option<String>> {
        if config.serve == RemoteServe::Off {
            return Ok(None);
        }
        let listener = lock(&self.listener);
        match &listener.public_url {
            Some(url) => Ok(Some(url.clone())),
            None => Err(Error::new(
                ErrorKind::Internal,
                format!(
                    "Tailscale HTTPS address not available yet: {}",
                    listener
                        .error
                        .as_deref()
                        .unwrap_or("remote access is not listening")
                ),
            )),
        }
    }

    /// Closes the connections of one device, or of every device for `None`.
    pub fn close(&self, device: Option<i64>) {
        let _ = self.closing.send(device);
    }

    pub fn closing(&self) -> broadcast::Receiver<Option<i64>> {
        self.closing.subscribe()
    }

    pub fn connected(&self, id: i64) {
        *lock(&self.connections).entry(id).or_default() += 1;
    }

    pub fn disconnected(&self, id: i64) {
        let mut connections = lock(&self.connections);
        if let Some(count) = connections.get_mut(&id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(&id);
            }
        }
    }

    pub fn connections(&self, id: i64) -> u32 {
        lock(&self.connections).get(&id).copied().unwrap_or(0)
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

pub fn load_config(paths: &Paths) -> Result<RemoteConfig> {
    Ok(Config::load(&paths.config())?.remote.unwrap_or_default())
}

fn save_config(paths: &Paths, config: &RemoteConfig) -> Result<()> {
    config
        .bind
        .parse::<IpAddr>()
        .map_err(|_| invalid(format!("invalid bind address {:?}", config.bind)))?;
    if config.serve != RemoteServe::Off && config.serve_port == 0 {
        return Err(invalid("the HTTPS port must not be 0".into()));
    }
    let _guard = lock(&SAVE_LOCK);
    let mut file = Config::load(&paths.config())?;
    file.remote = (config != &RemoteConfig::default()).then(|| config.clone());
    let text =
        toml::to_string(&file).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(&paths.config(), &text)
}

pub fn status(daemon: &Daemon) -> Result<RemoteStatus> {
    let config = load_config(daemon.paths())?;
    let listener = lock(&daemon.remote.listener);
    Ok(RemoteStatus {
        config,
        state: listener.state,
        address: listener.address.map(|a| a.to_string()),
        error: listener.error.clone(),
        public_url: listener.public_url.clone(),
        serve_command: listener.serve_command.clone(),
    })
}

pub fn set_config(daemon: &Daemon, config: &RemoteConfig) -> Result<()> {
    save_config(daemon.paths(), config)?;
    daemon.remote.close(None);
    daemon.remote.changed.notify_one();
    Ok(())
}

pub fn is_tailscale(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (o[1] & 0xc0) == 0x40
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

/// Tailscale addresses first, so the UI can preselect one.
pub fn interfaces() -> Result<Vec<RemoteInterface>> {
    let mut list: Vec<RemoteInterface> = if_addrs::get_if_addrs()?
        .into_iter()
        .map(|i| {
            let ip = i.ip();
            RemoteInterface {
                name: i.name,
                address: ip.to_string(),
                tailscale: is_tailscale(ip),
            }
        })
        .collect();
    list.push(RemoteInterface {
        name: "all".into(),
        address: "0.0.0.0".into(),
        tailscale: false,
    });
    list.sort_by_key(|i| !i.tailscale);
    Ok(list)
}

fn is_routable(ip: IpAddr) -> bool {
    let link_local = match ip {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
    };
    !ip.is_loopback() && !ip.is_unspecified() && !link_local
}

/// An unspecified bind (`0.0.0.0`) is not an address a phone can open, so the first usable interface stands in.
pub fn pairing_url(
    config: &RemoteConfig,
    bound: Option<SocketAddr>,
    interfaces: &[RemoteInterface],
    token: &str,
) -> String {
    let port = bound.map_or(config.port, |a| a.port());
    let configured = config
        .bind
        .parse::<IpAddr>()
        .unwrap_or(IpAddr::from([127, 0, 0, 1]));
    let ip = if configured.is_unspecified() {
        interfaces
            .iter()
            .filter_map(|i| i.address.parse::<IpAddr>().ok())
            .find(|ip| ip.is_ipv4() == configured.is_ipv4() && is_routable(*ip))
            .unwrap_or(configured)
    } else {
        configured
    };
    format!("http://{}/#token={token}", SocketAddr::new(ip, port))
}

fn device_info(daemon: &Daemon, stored: StoredRemoteDevice) -> RemoteDevice {
    RemoteDevice {
        connections: daemon.remote.connections(stored.id),
        id: stored.id,
        name: stored.name,
        created_at: stored.created_at,
        last_seen_at: stored.last_seen_at,
    }
}

struct PairingTarget {
    config: RemoteConfig,
    https_base: Option<String>,
}

fn pairing_target(daemon: &Daemon) -> Result<PairingTarget> {
    let config = load_config(daemon.paths())?;
    let https_base = daemon.remote.https_base(&config)?;
    Ok(PairingTarget { config, https_base })
}

fn pairing(
    daemon: &Daemon,
    target: &PairingTarget,
    stored: StoredRemoteDevice,
    token: &str,
) -> RemotePairing {
    let url = match &target.https_base {
        Some(base) => format!("{base}/#token={token}"),
        None => pairing_url(
            &target.config,
            daemon.remote.bound(),
            &interfaces().unwrap_or_default(),
            token,
        ),
    };
    RemotePairing {
        pairing_url: url,
        device: device_info(daemon, stored),
    }
}

fn not_found(id: i64) -> Error {
    Error::new(ErrorKind::NotFound, format!("no remote device {id}"))
}

pub fn add_device(daemon: &Daemon, name: &str) -> Result<RemotePairing> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("a device needs a name".into()));
    }
    let target = pairing_target(daemon)?;
    let token = token::generate()?;
    let stored = daemon
        .store()
        .add_remote_device(name, &token::hash(&token))?;
    Ok(pairing(daemon, &target, stored, &token))
}

pub fn list_devices(daemon: &Daemon) -> Result<Vec<RemoteDevice>> {
    let stored = daemon.store().remote_devices()?;
    Ok(stored.into_iter().map(|d| device_info(daemon, d)).collect())
}

pub fn rotate_device(daemon: &Daemon, id: i64) -> Result<RemotePairing> {
    let target = pairing_target(daemon)?;
    let token = token::generate()?;
    if !daemon
        .store()
        .set_remote_device_token(id, &token::hash(&token))?
    {
        return Err(not_found(id));
    }
    daemon.remote.close(Some(id));
    let stored = daemon
        .store()
        .remote_device(id)?
        .ok_or_else(|| not_found(id))?;
    Ok(pairing(daemon, &target, stored, &token))
}

pub fn remove_device(daemon: &Daemon, id: i64) -> Result<()> {
    if !daemon.store().remove_remote_device(id)? {
        return Err(not_found(id));
    }
    daemon.remote.close(Some(id));
    Ok(())
}

pub fn authenticate(daemon: &Daemon, token: &str) -> Result<Option<i64>> {
    let store = daemon.store();
    let id = store.remote_device_by_token(&token::hash(token))?;
    if let Some(id) = id {
        store.touch_remote_device(id)?;
    }
    Ok(id)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, address: &str) -> RemoteInterface {
        RemoteInterface {
            name: name.into(),
            address: address.into(),
            tailscale: is_tailscale(address.parse().unwrap()),
        }
    }

    #[test]
    fn detects_tailscale_addresses() {
        assert!(is_tailscale("100.101.102.103".parse().unwrap()));
        assert!(is_tailscale("100.64.0.1".parse().unwrap()));
        assert!(!is_tailscale("100.128.0.1".parse().unwrap()));
        assert!(!is_tailscale("192.168.1.2".parse().unwrap()));
        assert!(is_tailscale("fd7a:115c:a1e0::1".parse().unwrap()));
    }

    #[test]
    fn pairing_url_uses_the_bound_port() {
        let config = RemoteConfig {
            enabled: true,
            bind: "127.0.0.1".into(),
            port: 0,
            ..RemoteConfig::default()
        };
        let url = pairing_url(&config, Some("127.0.0.1:5555".parse().unwrap()), &[], "tok");
        assert_eq!(url, "http://127.0.0.1:5555/#token=tok");
    }

    #[test]
    fn pairing_url_replaces_unspecified_bind() {
        let config = RemoteConfig {
            enabled: true,
            bind: "0.0.0.0".into(),
            port: 7420,
            ..RemoteConfig::default()
        };
        let list = [
            iface("lo0", "127.0.0.1"),
            iface("en0", "192.168.1.2"),
            iface("utun4", "100.100.1.1"),
        ];
        let mut sorted = list.to_vec();
        sorted.sort_by_key(|i| !i.tailscale);
        assert_eq!(
            pairing_url(&config, None, &sorted, "t"),
            "http://100.100.1.1:7420/#token=t"
        );
    }

    #[test]
    fn pairing_url_skips_link_local() {
        let config = RemoteConfig {
            enabled: true,
            bind: "0.0.0.0".into(),
            port: 7420,
            ..RemoteConfig::default()
        };
        let list = [
            iface("lo0", "127.0.0.1"),
            iface("en0", "fe80::1"),
            iface("en0", "169.254.3.4"),
            iface("en0", "192.168.1.2"),
        ];
        assert_eq!(
            pairing_url(&config, None, &list, "t"),
            "http://192.168.1.2:7420/#token=t"
        );
    }

    #[test]
    fn pairing_url_brackets_ipv6() {
        let config = RemoteConfig {
            enabled: true,
            bind: "fd7a:115c:a1e0::1".into(),
            port: 7420,
            ..RemoteConfig::default()
        };
        assert_eq!(
            pairing_url(&config, None, &[], "t"),
            "http://[fd7a:115c:a1e0::1]:7420/#token=t"
        );
    }
}
