mod gway_srv;
mod util;
use std::net::SocketAddr;

use gway_srv::{H3_ADDR, TEST_HOST, TEST_RESPONSE_BODY};
use tokio::time::{Duration, sleep};

pub async fn get(url: &str, addr: SocketAddr) -> anyhow::Result<String> {
  dbg!(&url);
  let client = reqwest::Client::builder()
    .use_rustls_tls()
    .redirect(reqwest::redirect::Policy::none())
    .no_proxy()
    .http3_prior_knowledge()
    .min_tls_version(reqwest::tls::Version::TLS_1_3)
    .resolve(TEST_HOST, addr)
    .build()?;
  Ok(
    client
      .get(url)
      .version(http::Version::HTTP_3)
      .send()
      .await?
      .text()
      .await?,
  )
}

#[tokio::test]
async fn test_h3_proxy() -> anyhow::Result<()> {
  // sleep 1s to wait for the server to start
  sleep(Duration::from_secs(1)).await;
  let h3_addr: SocketAddr = H3_ADDR.parse()?;
  let url = format!("https://{TEST_HOST}");
  let body = get(&url, h3_addr).await?;
  println!("h3 body: {}", body);
  assert_eq!(body, TEST_RESPONSE_BODY);
  println!("h3 proxy test passed");

  Ok(())
}
