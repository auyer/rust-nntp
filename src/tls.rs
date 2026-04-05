//! TLS configuration and server address parsing.
//!
//! This module provides types for configuring TLS connections and parsing
//! NNTP server addresses with optional URL scheme support.
//!
//! # Address Formats
//!
//! `NNTPStream::connect` accepts addresses in several forms:
//!
//! | Format | Default Port | TLS |
//! |--------|-------------|-----|
//! | `"host:port"` | (required) | Auto-enabled if port is 563 |
//! | `"nntp://host"` | 119 | No |
//! | `"nntp://host:port"` | (from URL) | No |
//! | `"nntps://host"` | 563 | Yes |
//! | `"nntps://host:port"` | (from URL) | Yes |
//!
//! # Example
//!
//! ```no_run
//! use nntp::{NNTPStream, ServerAddress, TlsConfig};
//!
//! // Simple connect (port 563 auto-enables TLS)
//! let client = NNTPStream::connect("nntp.example.com:563".to_string());
//!
//! // Explicit TLS with certificate validation bypass (DANGEROUS)
//! let addr = ServerAddress::with_tls(
//!     "nntp.example.com",
//!     563,
//!     TlsConfig { danger_accept_invalid_certs: true },
//! );
//! let client = NNTPStream::connect_with(addr);
//! ```

use crate::address::ServerAddress;
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;

/// Builds a rustls `ClientConfig` using the platform's native certificate verifier.
pub(crate) fn build_client_config(tls_config: &TlsConfig) -> Result<ClientConfig, std::io::Error> {
    if tls_config.danger_accept_invalid_certs {
        build_dangerous_config()
    } else {
        ClientConfig::with_platform_verifier()
            .map_err(|e| std::io::Error::other(format!("failed to build TLS config: {e}")))
    }
}

/// TLS configuration for an NNTP connection.
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// Skip server certificate validation.
    ///
    /// **WARNING:** This makes the connection vulnerable to man-in-the-middle
    /// attacks. Only use for testing or when you understand the risks.
    pub danger_accept_invalid_certs: bool,
}

/// Builds a dangerous ClientConfig that skips certificate validation.
fn build_dangerous_config() -> Result<ClientConfig, std::io::Error> {
    use rustls::SignatureScheme;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, UnixTime};

    #[derive(Debug)]
    struct NoCertificateVerification;

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::ECDSA_NISTP521_SHA512,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
                SignatureScheme::ED25519,
            ]
        }
    }

    let verifier = Arc::new(NoCertificateVerification);

    Ok(ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth())
}

/// Wraps a TCP stream with TLS if configured, performing the handshake.
pub(crate) fn wrap_tls(
    tcp_stream: TcpStream,
    server_addr: &ServerAddress,
) -> Result<StreamOwned<ClientConnection, TcpStream>, std::io::Error> {
    let tls_config = server_addr
        .tls
        .as_ref()
        .expect("wrap_tls called but TLS is not configured");

    let config = build_client_config(tls_config)?;
    let server_name = ServerName::try_from(server_addr.host.as_str())
        .map_err(|e| std::io::Error::other(format!("invalid server name: {e}")))?
        .to_owned();

    let conn = ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| std::io::Error::other(format!("failed to create TLS connection: {e}")))?;

    let mut stream = StreamOwned::new(conn, tcp_stream);

    // Perform TLS handshake
    stream.conn.complete_io(&mut stream.sock)?;

    log::info!("TLS handshake completed");
    Ok(stream)
}
