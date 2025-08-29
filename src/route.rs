use std::{
  collections::{HashMap, HashSet},
  net::SocketAddr,
  sync::Arc,
};

use dashmap::{DashMap, mapref::one::Ref};
use faststr::FastStr;

#[derive(Debug, Clone)]
pub struct SiteConf {
  pub upstream: Arc<Upstream>,
  pub cert_host: FastStr,
}

impl SiteConf {
  pub fn new(upstream: Arc<Upstream>, cert_host: FastStr) -> Self {
    Self {
      upstream,
      cert_host,
    }
  }
}

#[derive(PartialEq, Eq, Debug)]
pub enum Protocol {
  H1,
}

#[derive(Debug)]
pub struct Upstream {
  pub addr_li: Box<[SocketAddr]>,
  pub connect_timeout_sec: u64,
  pub request_timeout_sec: u64,
  pub max_retry: usize,
  pub protocol: Protocol,
}

#[derive(Debug)]
pub struct UpstreamSiteSet {
  pub upstream: Arc<Upstream>,
  pub host_set: HashSet<FastStr>,
}

#[derive(Debug, Default)]
pub struct Route {
  pub host_conf: DashMap<FastStr, SiteConf>,
  pub upstream_site: HashMap<FastStr, UpstreamSiteSet>,
}

impl Route {
  pub fn add_upstream(&mut self, upstream_name: impl Into<FastStr>, upstream: Upstream) {
    let upstream_name = upstream_name.into();
    self.upstream_site.insert(
      upstream_name,
      UpstreamSiteSet {
        upstream: Arc::new(upstream),
        host_set: HashSet::new(),
      },
    );
  }

  pub fn set(
    &mut self,
    host: impl Into<FastStr>,
    cert_host: impl Into<FastStr>,
    upstream_name: impl Into<FastStr>,
  ) -> &mut Self {
    let upstream_name = upstream_name.into();
    let host = host.into();
    if let Some(t) = self.upstream_site.get_mut(&upstream_name) {
      t.host_set.insert(host.clone());
      self
        .host_conf
        .insert(host, SiteConf::new(t.upstream.clone(), cert_host.into()));
    }
    self
  }

  pub fn conf_by_host(&self, host: &str) -> Option<Ref<'_, FastStr, SiteConf>> {
    self.host_conf.get(host)
  }
}
