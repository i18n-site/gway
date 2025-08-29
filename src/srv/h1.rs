use std::sync::Arc;

use http_body_util::Full;
use hyper::{
  Request, Response, StatusCode, body::Bytes, header::HeaderValue, server::conn::http1,
  service::service_fn,
};
use hyper_util::rt::TokioIo;
use parking_lot::RwLock;
use sub_host::sub_host;
use tokio::net::TcpListener;

use crate::{Result, Route, req_host};

// 根据状态码生成响应
fn response(status: StatusCode) -> Response<Full<Bytes>> {
  let body = if status.is_redirection() {
    Bytes::new()
  } else {
    Bytes::from(status.canonical_reason().unwrap_or_default())
  };
  let mut res = Response::new(Full::new(body));
  *res.status_mut() = status;
  res
}

// 所有请求都跳转到 https
async fn redirect(
  req: Request<hyper::body::Incoming>,
  route: Arc<Route>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
  let host = req_host(&req);

  let pq = req
    .uri()
    .path_and_query()
    .map(|p| p.as_str())
    .unwrap_or("/");

  let host = if route.host_conf.contains_key(host) {
    host.to_owned()
  } else if let Some(h) = sub_host(host)
    && route.host_conf.contains_key(h.as_str())
  {
    h
  } else {
    return Ok(response(StatusCode::NOT_FOUND));
  };

  let new_uri = format!("https://{}{}", host, pq);
  Ok(match HeaderValue::from_str(&new_uri) {
    Ok(location) => {
      let mut res = response(StatusCode::MOVED_PERMANENTLY);
      res.headers_mut().insert("Location", location);
      res
    }
    Err(_) => response(StatusCode::INTERNAL_SERVER_ERROR),
  })
}

pub async fn srv(
  shutdown_lock: Arc<tokio::sync::RwLock<()>>,
  conn_lock: Arc<RwLock<()>>,
  listener: TcpListener,
  route: Arc<Route>,
) -> Result<()> {
  // let listener = TcpListener::bind(addr).await?;

  loop {
    tokio::select! {
        res = listener.accept() => {
            let (stream, _) = match res {
                Ok(val) => val,
                Err(e) => {
                    log::warn!("h1 accept error: {e}");
                    continue;
                }
            };

            let io = TokioIo::new(stream);
            let route = route.clone();
            let conn_lock = conn_lock.clone();

            tokio::spawn(
                #[allow(clippy::await_holding_lock)]
                async move {
                let _guard = conn_lock.read();
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service_fn(move |req| redirect(req, route.clone())))
                    .await
                {
                    log::warn!("h1: {:?}", err);
                }
            });
        },
        _ = shutdown_lock.read() => {
            break;
        }
    }
  }

  log::info!("h1 server shut down");
  Ok(())
}
