use std::net::SocketAddr;

use axum::{
  Router,
  http::StatusCode,
  response::{Html, IntoResponse},
  routing::get,
};

async fn handler_404() -> impl IntoResponse {
  (StatusCode::NOT_FOUND, "404 Not Found")
}

pub async fn axum_srv(addr: SocketAddr) -> anyhow::Result<()> {
  let app = Router::new()
    .route(
      "/",
      get(move || async move { Html(format!("Hello, World! from {}", addr)) }),
    )
    .route(
      "/pending",
      axum::routing::get(|| async {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        "pending"
      }),
    )
    .fallback(handler_404)
    .layer(tower_http::timeout::TimeoutLayer::new(
      std::time::Duration::from_secs(10),
    ));
  let listener = tokio::net::TcpListener::bind(addr).await?;
  axum::serve(listener, app)
    // .with_graceful_shutdown(async {
    //   // 10秒后关闭
    //   tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    // })
    .await?;
  Ok(())
}
