mod cert;
mod cert_loader;
mod error;
mod proxy;
mod route;
pub mod shutdown;
pub mod srv;

#[cfg(feature = "cert_dir")]
pub use cert_loader::CertDir;
pub use cert_loader::{CertLoad, CertLoader};
pub use error::{Error, IntoError, Result};
pub use proxy::proxy;
pub use route::{Protocol, Route, SiteConf, Upstream};
pub use srv::srv;

pub fn req_host<B>(req: &hyper::Request<B>) -> &str {
  req
    .headers()
    .get("host")
    .map(|h| h.to_str().unwrap_or_default())
    .unwrap_or_else(|| req.uri().host().unwrap_or_default())
}
