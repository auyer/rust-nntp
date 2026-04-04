use crate::tls::TlsConfig;

/// A resolved NNTP server address with optional TLS configuration.
///
/// Use [`ServerAddress::parse`] to parse a string address, or construct
/// directly with [`ServerAddress::new`] or [`ServerAddress::with_tls`].
#[derive(Debug, Clone)]
pub struct ServerAddress {
    /// The server hostname (used for TLS SNI and certificate validation).
    pub host: String,
    /// The server port number.
    pub port: u16,
    /// TLS configuration. `None` means plain TCP.
    pub tls: Option<TlsConfig>,
}

impl ServerAddress {
    /// Creates a plain (non-TLS) server address.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        ServerAddress {
            host: host.into(),
            port,
            tls: None,
        }
    }

    /// Creates a server address with TLS enabled.
    pub fn with_tls(host: impl Into<String>, port: u16, config: TlsConfig) -> Self {
        ServerAddress {
            host: host.into(),
            port,
            tls: Some(config),
        }
    }

    /// Parses a server address string.
    ///
    /// Supported formats:
    /// - `"nntps://host"` — TLS enabled, port 563
    /// - `"nntps://host:port"` — TLS enabled, custom port
    /// - `"nntp://host"` — no TLS, port 119
    /// - `"nntp://host:port"` — no TLS, custom port
    /// - `"host:port"` — no TLS, port 119
    ///
    /// # Errors
    ///
    /// Returns `"invalid address format"` if the string cannot be parsed.
    pub fn parse(addr: &str) -> Result<Self, &'static str> {
        if let Some(rest) = addr.strip_prefix("nntps://") {
            let (host, port) = parse_host_port(rest, 563)?;
            Ok(ServerAddress {
                host: host.to_owned(),
                port,
                tls: Some(TlsConfig::default()),
            })
        } else if let Some(rest) = addr.strip_prefix("nntp://") {
            let (host, port) = parse_host_port(rest, 119)?;
            Ok(ServerAddress {
                host: host.to_owned(),
                port,
                tls: None,
            })
        } else {
            // Bare host:port
            let (host, port) = parse_host_port(addr, 119)?;
            Ok(ServerAddress {
                host: host.to_owned(),
                port,
                tls: None,
            })
        }
    }
}

fn parse_host_port(s: &str, default_port: u16) -> Result<(&str, u16), &'static str> {
    if let Some((host, port_str)) = s.rsplit_once(':') {
        // Handle IPv6: [::1]:port
        if host.ends_with(']') {
            let port = port_str.parse::<u16>().map_err(|_| "invalid port number")?;
            return Ok((host, port));
        }
        let port = port_str.parse::<u16>().map_err(|_| "invalid port number")?;
        Ok((host, port))
    } else if default_port > 0 {
        Ok((s, default_port))
    } else {
        Err("port is required")
    }
}
