mod comm;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use comm::axum_srv;
use faststr::FastStr;
use gway::{CertDir, CertLoader, Protocol, Route, Upstream, shutdown, srv};

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  log_init::init();
  rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("failed to install aws-lc-rs as default provider");

  // 启动两个上游服务器用于测试
  let upstream_addr1: SocketAddr = "127.0.0.1:8081".parse()?;
  let upstream_addr2: SocketAddr = "127.0.0.1:8082".parse()?;
  tokio::spawn(axum_srv(upstream_addr1));
  tokio::spawn(axum_srv(upstream_addr2));

  // HTTP/1.1 代理地址
  let h1_addr: SocketAddr = "0.0.0.0:8080".parse()?;
  let h2_addr: SocketAddr = "0.0.0.0:9083".parse()?;
  let h3_addr: SocketAddr = "0.0.0.0:9083".parse()?;

  let mut route = Route::default();
  let upstream = Upstream {
    addr_li: vec![upstream_addr1, upstream_addr2].into_boxed_slice(),
    connect_timeout_sec: 10,
    request_timeout_sec: 10,
    max_retry: 3,
    protocol: Protocol::H1,
  };

  let upstream_name = FastStr::from("test_upstream");

  route.add_upstream(upstream_name.clone(), upstream);
  route.set("test.018007.xyz", "018007.xyz", upstream_name.clone());
  route.set("018007.xyz", "018007.xyz", upstream_name);

  let cert_db = CertDir {
    base: PathBuf::from(MANIFEST_DIR).join("examples/ssl"),
  };
  let cert_loader = CertLoader::new(cert_db);

  srv(
    Arc::new(route),
    cert_loader,
    shutdown::signal(),
    h1_addr,
    h2_addr,
    h3_addr,
  )
  .await?;

  Ok(())
}
