#![allow(dead_code)]

use std::net::SocketAddr;

use reqwest::{Client, ClientBuilder, Response};

pub fn build(
  host: &str,
  addr: SocketAddr,
  builder: impl FnOnce(ClientBuilder) -> ClientBuilder,
) -> anyhow::Result<Client> {
  let client_builder = builder(reqwest::Client::builder()).use_rustls_tls();

  let client = client_builder
    .redirect(reqwest::redirect::Policy::none())
    .resolve(host, addr)
    .no_proxy()
    .build()?;
  Ok(client)
}

pub async fn get_with_builder(
  url_str: &str,
  addr: SocketAddr,
  builder: impl FnOnce(ClientBuilder) -> ClientBuilder,
) -> anyhow::Result<Response> {
  let url = url::Url::parse(url_str)?;
  let host = url
    .host_str()
    .ok_or_else(|| anyhow::anyhow!("URL does not have a host"))?;
  let client = build(host, addr, builder)?;
  client.get(url_str).send().await.map_err(Into::into)
}

pub async fn get(url_str: &str, addr: SocketAddr) -> anyhow::Result<Response> {
  get_with_builder(url_str, addr, |c| c).await
}

pub async fn get_body(url_str: &str, addr: SocketAddr) -> anyhow::Result<String> {
  let res = get(url_str, addr).await?;
  res.text().await.map_err(Into::into)
}

pub async fn get_body_h2(url_str: &str, addr: SocketAddr) -> anyhow::Result<String> {
  let res = get_with_builder(url_str, addr, |c| c.http2_prior_knowledge()).await?;
  res.text().await.map_err(Into::into)
}

pub async fn get_response_h2(url_str: &str, addr: SocketAddr) -> anyhow::Result<Response> {
  get_with_builder(url_str, addr, |c| c.http2_prior_knowledge()).await
}

pub async fn get_body_h3(url_str: &str, addr: SocketAddr) -> anyhow::Result<String> {
  let res = get_with_builder(url_str, addr, |c| c.http3_prior_knowledge()).await?;
  res.text().await.map_err(Into::into)
}

pub async fn post_with_builder(
  url_str: &str,
  addr: SocketAddr,
  body: impl Into<reqwest::Body>,
  builder: impl FnOnce(ClientBuilder) -> ClientBuilder,
) -> anyhow::Result<Response> {
  let url = url::Url::parse(url_str)?;
  let host = url
    .host_str()
    .ok_or_else(|| anyhow::anyhow!("URL does not have a host"))?;

  let client_builder = builder(reqwest::Client::builder()).use_rustls_tls();

  let client = client_builder
    .redirect(reqwest::redirect::Policy::none())
    .resolve(host, addr)
    .no_proxy()
    .build()?;

  client
    .post(url_str)
    .body(body)
    .send()
    .await
    .map_err(Into::into)
}

pub async fn post_body_h2(
  url_str: &str,
  addr: SocketAddr,
  body: impl Into<reqwest::Body>,
) -> anyhow::Result<String> {
  let res = post_with_builder(url_str, addr, body, |c| c.http2_prior_knowledge()).await?;
  res.text().await.map_err(Into::into)
}
