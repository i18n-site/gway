use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use coarsetime::Clock;
use dashmap::DashMap;
use faststr::FastStr;
use parking_lot::RwLock;
use sub_host::sub_host;
use tokio::time;
use x509_parser::{extensions::GeneralName, parse_x509_certificate};

use crate::{
  cert::Cert,
  error::{Error, Result},
};

#[cfg(feature = "cert_dir")]
mod dir;

#[cfg(feature = "cert_dir")]
pub use dir::CertDir;

pub trait CertLoad: Send + Sync + 'static + Debug {
  fn load(
    &self,
    host: impl Into<FastStr> + Send + Sync,
  ) -> impl std::future::Future<Output = Result<Option<Arc<Cert>>>> + Send + Sync;
}

#[derive(Debug)]
pub struct CertLoader<L: CertLoad> {
  // 证书缓存
  pub host_cert: DashMap<FastStr, Arc<Cert>>,
  // 证书过期时间
  pub expire: RwLock<BTreeMap<i64, Vec<FastStr>>>,
  // 证书加载器
  pub loader: L,
}

impl<L: CertLoad> CertLoader<L> {
  pub fn new(loader: L) -> Arc<Self> {
    let s = Arc::new(Self {
      host_cert: DashMap::new(),
      expire: RwLock::new(BTreeMap::new()),
      loader,
    });

    let s2 = s.clone();
    tokio::spawn(async move {
      let mut interval = time::interval(time::Duration::from_secs(86400));
      loop {
        interval.tick().await;
        s2.rm_expired(2);
      }
    });

    s
  }

  pub fn rm_expired(&self, days: i64) {
    let mut to_rm_expire = Vec::new();
    for (expire, hosts) in self.expire.read().iter() {
      if *expire < (Clock::now_since_epoch().as_secs() as i64) + days * 24 * 60 * 60 {
        for host in hosts {
          self.host_cert.remove(host);
        }
        to_rm_expire.push(*expire);
      } else {
        break;
      }
    }

    if !to_rm_expire.is_empty() {
      let mut expire_write = self.expire.write();
      for expire in to_rm_expire {
        expire_write.remove(&expire);
      }
    }
  }

  async fn cert_by_host(&self, host: impl Into<FastStr>) -> Result<Option<Arc<Cert>>> {
    let h = host.into();
    if let Some(c) = self.host_cert.get::<FastStr>(&h) {
      return Ok(Some(c.clone()));
    }

    if let Some(cert) = self.loader.load(h.clone()).await? {
      let (_, cert_pem) = parse_x509_certificate(&cert.rustls.fullchain[0])
        .map_err(|e| Error::CertParse(format!("证书解析失败: {e:?}")))?;
      let cert_expire = cert_pem.validity().not_after.timestamp() / 86400;
      let mut cert_host_li = Vec::new();
      if let Some(san) = cert_pem.subject_alternative_name().unwrap() {
        for name in &san.value.general_names {
          if let GeneralName::DNSName(domain) = name {
            cert_host_li.push(FastStr::from(domain.to_string()));
          }
        }
      }
      // 找到最短的域名
      if let Some(shortest_host) = cert_host_li.iter().min_by_key(|h| h.len()) {
        self.host_cert.insert(shortest_host.clone(), cert.clone());
        self
          .expire
          .write()
          .entry(cert_expire)
          .or_default()
          .push(shortest_host.clone());
      }
      return Ok(Some(cert));
    }
    Ok(None)
  }

  pub async fn get(&self, host: impl Into<String>) -> Result<Arc<Cert>> {
    let host = host.into();

    if let Some(cert) = self.cert_by_host(host.clone()).await? {
      return Ok(cert);
    }

    if let Some(sub) = sub_host(&host)
      && let Some(cert) = self.cert_by_host(sub).await?
    {
      return Ok(cert);
    }
    Err(Error::CertNotFound(host))
  }
}
