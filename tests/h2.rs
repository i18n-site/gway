mod comm;
mod gway_srv;
mod util;
use std::{net::SocketAddr, sync::Arc};

use comm::randstr;
use gway_srv::{TEST_HOST, TEST_RESPONSE_BODY};
use reqwest::StatusCode;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn test_h2_proxy() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let path = "/";
  let h2_addr: SocketAddr = gway_srv::H2_ADDR.parse()?;
  let url = format!("https://{TEST_HOST}{path}");

  let body = util::get_body_h2(&url, h2_addr).await?;

  assert_eq!(body, TEST_RESPONSE_BODY);

  // 这里会出错
  let body = util::get_body(&url, h2_addr).await?;
  assert_eq!(body, TEST_RESPONSE_BODY);

  Ok(())
}

#[tokio::test]
async fn test_h2_subdomain_redirect() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let path = "/";
  let h2_addr: SocketAddr = gway_srv::H2_ADDR.parse()?;

  let res = util::get_response_h2(&format!("https://sub.{TEST_HOST}{path}"), h2_addr).await?;

  assert_eq!(res.status(), StatusCode::MOVED_PERMANENTLY);
  assert_eq!(
    res.headers().get("location").unwrap(),
    &format!("https://{TEST_HOST}{path}")
  );

  Ok(())
}

#[tokio::test]
async fn test_h2_not_found() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let path = "/404";
  let h2_addr: SocketAddr = gway_srv::H2_ADDR.parse()?;
  let url = format!("https://{TEST_HOST}{path}");

  let res = util::get_response_h2(&url, h2_addr).await?;

  assert_eq!(res.status(), StatusCode::NOT_FOUND);

  Ok(())
}

#[tokio::test]
async fn test_h2_post() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let path = "/";
  let h2_addr: SocketAddr = gway_srv::H2_ADDR.parse()?;
  let url = format!("https://{TEST_HOST}{path}");
  let post_body = "Hello, from POST!";

  let body = util::post_body_h2(&url, h2_addr, post_body).await?;

  assert_eq!(body, post_body);

  Ok(())
}

#[tokio::test]
async fn test_h2_post_1mb() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let path = "/";
  let h2_addr: SocketAddr = gway_srv::H2_ADDR.parse()?;
  let url = format!("https://{TEST_HOST}{path}");
  let post_body = randstr(1024 * 1024);

  let body = util::post_body_h2(&url, h2_addr, post_body.clone()).await?;

  assert_eq!(body.len(), post_body.len());
  assert_eq!(body, post_body);

  Ok(())
}

#[tokio::test]
async fn test_h2_concurrent_requests() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let path = "/pending";
  let h2_addr: SocketAddr = gway_srv::H2_ADDR.parse()?;
  let url = format!("https://{TEST_HOST}{path}");
  let client = Arc::new(util::build(TEST_HOST, h2_addr, |c| {
    c.http2_prior_knowledge()
  })?);
  let mut handles = Vec::new();

  for _ in 0..100 {
    let url = url.clone();
    let client = client.clone();
    handles.push(tokio::spawn(async move {
      client.get(&url).send().await?.text().await
    }));
  }

  for handle in handles {
    let res = handle.await??;
    assert_eq!(res, "pending");
  }

  Ok(())
}
