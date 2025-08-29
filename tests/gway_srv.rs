use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use gway::{CertDir, CertLoader, Protocol, Route, SiteConf, Upstream};
use static_init::constructor;

pub const TEST_HOST: &str = "018007.xyz";
pub const H1_ADDR: &str = "0.0.0.0:9081";
pub const H2_ADDR: &str = "0.0.0.0:9082";
pub const H3_ADDR: &str = "127.0.0.1:9083";
pub const UPSTREAM_ADDR: &str = "127.0.0.1:9080";
pub const TEST_RESPONSE_BODY: &str = "Hello, from upstream!";

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[constructor(0)]
extern "C" fn start_server() {
  log_init::init();

  rustls::crypto::ring::default_provider()
    .install_default()
    .unwrap();
  std::thread::spawn(|| {
    let rt = tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .build()
      .unwrap();

    rt.block_on(async {
      // 启动上游服务
      tokio::spawn(async move {
        // axum app
        let app = axum::Router::new()
          .route(
            "/",
            axum::routing::get(|| async { TEST_RESPONSE_BODY })
              .post(|body: axum::body::Bytes| async { body }),
          )
          .route(
            "/pending",
            axum::routing::get(|| async {
              tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
              "pending"
            }),
          );
        // 尝试绑定端口，如果失败则跳过（可能已经在运行）
        match tokio::net::TcpListener::bind(UPSTREAM_ADDR).await {
          Ok(listener) => {
            println!("Upstream server started on {}", UPSTREAM_ADDR);
            axum::serve(listener, app).await.unwrap();
          }
          Err(e) => {
            println!(
              "Upstream server already running or failed to bind {}: {}",
              UPSTREAM_ADDR, e
            );
          }
        }
      });

      // 配置 gway 服务
      let db = CertDir {
        base: PathBuf::from(MANIFEST_DIR).join("examples/ssl"),
      };
      let host = "018007.xyz";
      let route = Route::default();
      let upstream = Upstream {
        addr_li: vec![UPSTREAM_ADDR.parse().unwrap()].into(),
        connect_timeout_sec: 5,
        request_timeout_sec: 10,
        max_retry: 3,
        protocol: Protocol::H1,
      };
      let site_conf = SiteConf::new(Arc::new(upstream), host.into());
      route.host_conf.insert(host.into(), site_conf);

      let h1_addr: SocketAddr = H1_ADDR.parse().unwrap();
      let h2_addr: SocketAddr = H2_ADDR.parse().unwrap();
      let h3_addr: SocketAddr = H3_ADDR.parse().unwrap();

      let cert_loader = CertLoader::new(db);
      let route = Arc::new(route);
      // 在后台运行 gway 服务
      if let Err(e) = gway::srv(
        route,
        cert_loader,
        std::future::pending(),
        h1_addr,
        h2_addr,
        h3_addr,
      )
      .await
      {
        eprintln!("Failed to run server: {}", e);
      }
    });
  });
}
