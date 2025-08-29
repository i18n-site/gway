use std::{path::PathBuf, sync::Arc};

use faststr::FastStr;

use super::CertLoad;
use crate::{
  cert::Cert,
  error::{Error, Result},
};

#[derive(Debug, Clone)]
pub struct CertDir {
  pub base: PathBuf,
}

impl CertLoad for CertDir {
  async fn load(&self, host: impl Into<FastStr> + Send + Sync) -> Result<Option<Arc<Cert>>> {
    let host = host.into();
    // 证书和私钥的路径
    let cert_path = self.base.join(format!("{host}_ecc/fullchain.cer"));
    let key_path = self.base.join(format!("{host}_ecc/{host}.key"));

    if !tokio::fs::try_exists(&cert_path).await.unwrap_or(false)
      || !tokio::fs::try_exists(&key_path).await.unwrap_or(false)
    {
      return Ok(None);
    }

    // 异步读取文件内容
    let cert_str = tokio::fs::read_to_string(cert_path)
      .await
      .map_err(Error::Io)?;
    let key_str = tokio::fs::read_to_string(key_path)
      .await
      .map_err(Error::Io)?;

    Cert::new(cert_str, key_str).map(|cert| Some(Arc::new(cert)))
  }
}
