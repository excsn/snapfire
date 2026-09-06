//! `[server.tls]`: the certificate the listener presents, re-read and swapped
//! in place when the configured signal arrives. Behind the `tls` feature.

#![cfg(feature = "tls")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

use crate::HostError;

/// A loaded certificate and the files it came from. `acceptor` hands out the
/// one in force; `reload` replaces it for the handshakes that follow.
pub struct Tls {
  cert: PathBuf,
  key: PathBuf,
  alpn: Vec<Vec<u8>>,
  current: parking_lot::RwLock<Arc<ServerConfig>>,
}

impl Tls {
  pub fn load(cert: PathBuf, key: PathBuf, alpn: Vec<String>) -> Result<Tls, HostError> {
    let alpn: Vec<Vec<u8>> = alpn.into_iter().map(String::into_bytes).collect();
    let config = read(&cert, &key, &alpn)?;
    Ok(Tls { cert, key, alpn, current: parking_lot::RwLock::new(Arc::new(config)) })
  }

  pub fn acceptor(&self) -> tokio_rustls::TlsAcceptor {
    tokio_rustls::TlsAcceptor::from(self.current.read().clone())
  }

  /// Re-reads both files and swaps what the next handshake presents.
  /// Connections already up keep the certificate they started on, and a file
  /// that will not read leaves the running one in place.
  pub fn reload(&self) -> Result<(), HostError> {
    let config = read(&self.cert, &self.key, &self.alpn)?;
    *self.current.write() = Arc::new(config);
    Ok(())
  }

  pub fn cert(&self) -> &Path {
    &self.cert
  }

  pub fn key(&self) -> &Path {
    &self.key
  }

  /// What the handshake offers, for the report.
  pub fn alpn(&self) -> Vec<String> {
    self.alpn.iter().map(|p| String::from_utf8_lossy(p).into_owned()).collect()
  }
}

fn read(cert: &Path, key: &Path, alpn: &[Vec<u8>]) -> Result<ServerConfig, HostError> {
  let at = |path: &Path, e: String| HostError::Config(path.to_path_buf(), e);
  let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
    .map_err(|e| at(cert, e.to_string()))?
    .collect::<Result<_, _>>()
    .map_err(|e| at(cert, e.to_string()))?;
  if chain.is_empty() {
    return Err(at(cert, "no certificate in the file".to_owned()));
  }
  let key = PrivateKeyDer::from_pem_file(key).map_err(|e| at(key, e.to_string()))?;
  let provider = Arc::new(rustls::crypto::ring::default_provider());
  let mut config = ServerConfig::builder_with_provider(provider)
    .with_safe_default_protocol_versions()
    .map_err(|e| at(cert, e.to_string()))?
    .with_no_client_auth()
    .with_single_cert(chain, key)
    .map_err(|e| at(cert, e.to_string()))?;
  config.alpn_protocols = alpn.to_vec();
  Ok(config)
}
