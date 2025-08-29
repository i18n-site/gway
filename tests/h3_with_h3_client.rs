mod gway_srv;
mod util;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use bytes::Buf;
use gway_srv::{TEST_HOST, TEST_RESPONSE_BODY};
use http::{Method, Request, Uri};
use static_init::dynamic;
use tokio::time::sleep;

#[dynamic]
static CLIENT_CONFIG: quinn::ClientConfig = {
  // 创建标准的 TLS 配置，使用 webpki 根证书
  let mut root_store = rustls::RootCertStore::empty();
  root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

  let mut tls_config = rustls::ClientConfig::builder()
    .with_root_certificates(root_store)
    .with_no_client_auth();
  tls_config.alpn_protocols = vec![b"h3".to_vec()];

  quinn::ClientConfig::new(Arc::new(
    quinn::crypto::rustls::QuicClientConfig::try_from(tls_config).unwrap(),
  ))
};

/// 使用 h3-quinn 客户端发送 H3 请求，使用真实证书验证
async fn send_h3_request_with_quinn(
  addr: SocketAddr,
  host: &str,
  path: &str,
) -> Result<(http::StatusCode, http::HeaderMap, String)> {
  // 等待服务器启动
  sleep(Duration::from_secs(1)).await;

  let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
  endpoint.set_default_client_config(CLIENT_CONFIG.clone());

  // 连接到服务器，使用正确的主机名进行证书验证
  let connection = endpoint.connect(addr, host)?.await?;

  // 创建 H3 连接
  let h3_conn = h3_quinn::Connection::new(connection);
  let (_driver, mut send_request) = h3::client::new(h3_conn).await?;

  let uri: Uri = format!("https://{}{}", host, path).parse()?;

  let req = Request::builder()
    .method(Method::GET)
    .uri(uri)
    .header("host", host)
    .body(())?;

  let mut stream = send_request.send_request(req).await?;
  stream.finish().await?;

  let resp = stream.recv_response().await?;
  println!("H3 Response status: {}", resp.status());

  let status = resp.status();
  let headers = resp.headers().clone();

  let mut body = Vec::new();
  while let Some(mut chunk) = stream.recv_data().await? {
    let chunk_bytes = chunk.copy_to_bytes(chunk.remaining());
    body.extend_from_slice(&chunk_bytes);
  }

  Ok((status, headers, String::from_utf8(body)?))
}

#[tokio::test]
async fn test_h3_quinn_client() -> Result<()> {
  let h3_addr: SocketAddr = gway_srv::H3_ADDR.parse()?;
  let path = "/";

  // 发送请求
  let (_status, _headers, body) = send_h3_request_with_quinn(h3_addr, TEST_HOST, path).await?;

  assert_eq!(body, TEST_RESPONSE_BODY);

  Ok(())
}
