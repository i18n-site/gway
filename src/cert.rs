use std::sync::Arc;

use rustls_pemfile::{certs, private_key};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use s2n_quic::provider::tls::s2n_tls::config::Config;

use crate::{Error, Result};

#[derive(Debug)]
pub struct Rusttls {
  pub fullchain: Vec<CertificateDer<'static>>,
  pub key: PrivateKeyDer<'static>,
}

#[derive(Debug, Clone)]
pub struct Cert {
  pub s2n: Arc<Config>,
  pub rustls: Arc<Rusttls>,
}

impl Cert {
  pub fn new(fullchain: impl Into<String>, key: impl Into<String>) -> Result<Self> {
    let fullchain = fullchain.into();
    let key = key.into();
    let mut cert_reader = std::io::Cursor::new(fullchain.as_bytes());

    let mut key_reader = std::io::Cursor::new(key.as_bytes());

    let tls_config = s2n_quic::provider::tls::s2n_tls::Server::builder()
      .with_application_protocols(&[b"h3".to_vec()])?
      .with_certificate(fullchain.clone(), key.clone())?
      .build()?;

    let rustls = Arc::new(Rusttls {
      fullchain: certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::CertParse(format!("证书解析失败: {e:?}")))?,
      key: private_key(&mut key_reader)
        .map_err(|e| Error::CertParse(format!("私钥解析失败: {e:?}")))?
        .ok_or(Error::PrivateKeyNotFound)?,
    });

    Ok(Cert {
      rustls,
      s2n: Arc::new(tls_config.into()),
    })
  }
}
