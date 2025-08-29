mod comm;
mod gway_srv;
mod util;
use std::{net::SocketAddr, time::Duration};

use anyhow::Result;
use bytes::{Buf, Bytes};
use comm::randstr;
use gway::srv::s2n_quic as gway_s2n_quic;
use gway_srv::{TEST_HOST, TEST_RESPONSE_BODY};
use http::{Method, Request, Uri};
use s2n_quic::Client;
use tokio::time::sleep;

/// 使用 s2n-quic 客户端发送 H3 请求
async fn send_h3_request(
  addr: SocketAddr,
  host: &str,
  path: &str,
  method: Method,
  body_data: Option<Bytes>,
) -> Result<(http::StatusCode, http::HeaderMap, String)> {
  sleep(Duration::from_secs(1)).await;

  let tls = s2n_quic::provider::tls::s2n_tls::Client::builder().build()?;

  let client = Client::builder()
    .with_tls(tls)?
    .with_io("0.0.0.0:0")?
    .start()?;

  let connect_to = s2n_quic::client::Connect::new(addr).with_server_name(TEST_HOST);
  let mut connection = client.connect(connect_to).await?;

  // 等待连接建立
  connection.keep_alive(true)?;

  // 创建 H3 连接
  let h3_conn = gway_s2n_quic::Connection::new(connection);
  let (_driver, mut send_request) = h3::client::new(h3_conn).await?;

  let uri: Uri = format!("https://{}{}", host, path).parse()?;

  let req = Request::builder()
    .method(method)
    .uri(uri)
    .header("host", host)
    .body(())?;

  let mut stream = send_request.send_request(req).await?;

  if let Some(data) = body_data {
    stream.send_data(data).await?;
  }

  stream.finish().await?;

  let resp = stream.recv_response().await?;

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
async fn test_h3_s2n_client() -> Result<()> {
  let h3_addr: SocketAddr = gway_srv::H3_ADDR.parse()?;
  let path = "/";

  // 发送请求
  let (_status, _headers, body) =
    send_h3_request(h3_addr, TEST_HOST, path, Method::GET, None).await?;

  assert_eq!(body, TEST_RESPONSE_BODY);

  Ok(())
}

#[tokio::test]
async fn test_h3_post() -> Result<()> {
  let h3_addr: SocketAddr = gway_srv::H3_ADDR.parse()?;
  let path = "/";
  let post_body = "Hello, from POST!";

  // 发送请求
  let (_status, _headers, body) = send_h3_request(
    h3_addr,
    TEST_HOST,
    path,
    Method::POST,
    Some(Bytes::from_static(post_body.as_bytes())),
  )
  .await?;

  assert_eq!(body, post_body);

  Ok(())
}

#[tokio::test]
async fn test_h3_post_1mb() -> Result<()> {
  let h3_addr: SocketAddr = gway_srv::H3_ADDR.parse()?;
  let path = "/";
  let post_body = randstr(1024 * 1024);

  // 发送请求
  let (_status, _headers, body) = send_h3_request(
    h3_addr,
    TEST_HOST,
    path,
    Method::POST,
    Some(Bytes::from(post_body.clone())),
  )
  .await?;

  assert_eq!(body.len(), post_body.len());
  assert_eq!(body, post_body);

  Ok(())
}

#[tokio::test]
async fn test_h3_subdomain_redirect() -> Result<()> {
  let h3_addr: SocketAddr = gway_srv::H3_ADDR.parse()?;
  let path = "/";
  let subdomain_host = &format!("sub.{}", TEST_HOST);

  // 发送请求
  let (status, headers, _body) =
    send_h3_request(h3_addr, subdomain_host, path, Method::GET, None).await?;

  assert_eq!(status, http::StatusCode::MOVED_PERMANENTLY);
  assert_eq!(
    headers.get("location").unwrap(),
    &format!("https://{}{}", TEST_HOST, path)
  );

  Ok(())
}

#[tokio::test]
async fn test_h3_not_found() -> Result<()> {
  let h3_addr: SocketAddr = gway_srv::H3_ADDR.parse()?;
  let path = "/404";

  let (status, _headers, _body) =
    send_h3_request(h3_addr, TEST_HOST, path, Method::GET, None).await?;

  assert_eq!(status, http::StatusCode::NOT_FOUND);

  Ok(())
}
