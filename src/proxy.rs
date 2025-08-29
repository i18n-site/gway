use std::sync::Arc;

use http::{Request, Response, StatusCode, header, response::Builder};
use http_body::Body;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Bytes;
use sub_host::sub_host;

use crate::{Error, IntoError, Result, Route, req_host, route::Protocol::H1};

pub static mut N: usize = 0;

pub async fn proxy<B>(req: Request<B>, route: Arc<Route>) -> Response<BoxBody<Bytes, hyper::Error>>
where
  B: Body<Data = Bytes> + Send + 'static,
  B::Error: IntoError + Send + Sync + 'static,
{
  let host = req_host(&req).to_owned();
  let path = req
    .uri()
    .path_and_query()
    .map(|x| x.as_str())
    .unwrap_or("")
    .to_owned();
  match _proxy(&host, &path, req, route).await {
    Ok(res) => {
      let status = res.status();
      log::info!("{status} {host} {path}");
      res
    }
    Err(err) => {
      let err = err.to_string();
      log::warn!("Error: {host} {path} {err}");
      response(|b| b.status(500), err).unwrap_or_default()
    }
  }
}

fn response(
  build: impl Fn(Builder) -> Builder,
  body: impl Into<Bytes>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>> {
  Ok(
    build(Builder::new()).body(
      Full::new(body.into())
        .map_err(|never| match never {})
        .boxed(),
    )?,
  )
}

pub async fn _proxy<B>(
  host: &str,
  path_and_query: &str,
  req: Request<B>,
  route: Arc<Route>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>>
where
  B: Body<Data = Bytes> + Send + 'static,
  B::Error: IntoError + Send + Sync + 'static,
{
  if let Some(site_conf) = route.host_conf.get(host) {
    let site_conf = site_conf.value();
    let upstream = &site_conf.upstream;
    let protocol = &upstream.protocol;
    let upstream_addr_li = &upstream.addr_li;
    let len = upstream_addr_li.len();
    if len == 0 {
      return Err(Error::UpstreamNotFound);
    }
    let (mut parts, body) = req.into_parts();

    match protocol {
      H1 => {
        parts.version = http::Version::HTTP_11;
        parts.headers.insert(
          header::CONNECTION,
          header::HeaderValue::from_static("keep-alive"),
        );
      }
    }
    let body = body.collect().await.map_err(|e| e.into_error())?.to_bytes();
    let mut pos = unsafe {
      N = N.overflowing_add(1).0;
      N
    } % len;
    let mut retry = 0;
    loop {
      let upstream_addr = upstream_addr_li[pos];
      let req = Request::from_parts(parts.clone(), Full::new(body.clone()));
      let r = match protocol {
        H1 => pooled_fetch::http(upstream_addr, req).await,
      };
      match r {
        Ok(res) => {
          return Ok(res.map(|b| b.boxed()));
        }
        Err(err) => {
          log::warn!("Error: {host} {path_and_query} {upstream_addr} {}", err);
          retry += 1;
          if retry > upstream.max_retry {
            return Err(err.into());
          }
          pos = (pos + 1) % len;
        }
      }
    }
  } else {
    if let Some(host) = sub_host(host)
      && route.host_conf.get(&faststr::FastStr::new(&host)).is_some()
    {
      return response(
        |b| {
          let new_uri = format!("https://{}{}", host, path_and_query);
          b.status(StatusCode::MOVED_PERMANENTLY)
            .header(header::LOCATION, new_uri)
        },
        &b""[..],
      );
    }
    response(|b| b.status(404), &b"404: Not Found"[..])
  }
}
