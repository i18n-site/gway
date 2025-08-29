use s2n_quic::provider::StartError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("CertParse: {0}")]
  CertParse(String),

  #[error("CertNotFound")]
  CertNotFound(String),

  #[error("SniMissing")]
  SniMissing,

  #[error("CertExpired")]
  CertExpired,

  #[error("PrivateKeyNotFound")]
  PrivateKeyNotFound,

  #[error("PrivateKeyUnsupported: {0}")]
  PrivateKeyUnsupported(String),

  #[error("io: {0}")]
  Io(#[from] std::io::Error),

  #[error("http: {0}")]
  Http(#[from] hyper::http::Error),

  #[error("Rustls: {0}")]
  Rustls(#[from] rustls::Error),

  #[error("NoHost")]
  NoHost,

  #[error("UpstreamNotFound")]
  UpstreamNotFound,

  #[error("InvalidHost: {0}")]
  InvalidHost(#[from] hyper::http::uri::InvalidUri),

  #[error("TokioJoin: {0}")]
  TokioJoin(#[from] tokio::task::JoinError),

  #[error("Infallible: {0}")]
  Infallible(#[from] std::convert::Infallible),

  #[error("Hyper: {0}")]
  Hyper(#[from] hyper::Error),

  #[error("H3Connection: {0}")]
  H3Connection(#[from] h3::error::ConnectionError),

  #[error("H3Stream: {0}")]
  H3Stream(#[from] h3::error::StreamError),

  #[error("S2nQuicStart: {0}")]
  S2nQuicStart(#[from] StartError),

  #[error("S2nQuicTls: {0}")]
  S2nQuicTls(#[from] s2n_quic::provider::tls::s2n_tls::error::Error),

  #[error("pooled_fetch: {0}")]
  PooledFetch(#[from] pooled_fetch::Error),

  #[error("H3: {0}")]
  H3(String),

  #[error("ListenerNotFound: {0}")]
  ListenerNotFound(std::net::SocketAddr),
}

pub trait IntoError {
  fn into_error(self) -> Error;
}

impl IntoError for hyper::Error {
  fn into_error(self) -> Error {
    Error::Hyper(self)
  }
}

impl IntoError for std::convert::Infallible {
  fn into_error(self) -> Error {
    Error::Infallible(self)
  }
}

impl IntoError for Error {
  fn into_error(self) -> Error {
    self
  }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
