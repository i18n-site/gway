use std::{net::UdpSocket, pin::Pin, sync::Arc};

use bytes::{Buf, Bytes};
use futures_util::task::Poll;
use h3::server::RequestStream;
use http_body::Frame;
use http_body_util::BodyExt;
use hyper::{Request, Response};
use parking_lot::RwLock;
use s2n_quic::{
  Connection as QuicConnection, Server,
  provider::tls::s2n_tls::{
    callbacks::{ClientHelloCallback, ConfigResolver, ConnectionFuture},
    connection::Connection as S2nTlsConnection,
    error::Error as S2nError,
  },
};

use super::s2n_quic as h3_quic;
use crate::{CertLoad, CertLoader, Error, Result, proxy, route::Route};

struct Cert<D: CertLoad> {
  cert_loader: Arc<CertLoader<D>>,
}

impl<D: CertLoad> ClientHelloCallback for Cert<D> {
  fn on_client_hello(
    &self,
    connection: &mut S2nTlsConnection,
  ) -> Result<Option<Pin<Box<dyn ConnectionFuture>>>, S2nError> {
    let sni = match connection.server_name() {
      Some(sni) => sni.to_string(),
      None => {
        return Err(S2nError::application(Box::new(Error::CertNotFound(
          "sni not found".to_string(),
        ))));
      }
    };

    let cert_loader = self.cert_loader.clone();

    let fut = async move {
      let cert = cert_loader.get(&sni).await.map_err(|e| {
        S2nError::application(Box::new(Error::CertNotFound(format!(
          "cert not found for {sni}: {e}"
        ))))
      })?;

      Ok(cert.s2n.as_ref().clone())
    };

    Ok(Some(Box::pin(ConfigResolver::new(fut))))
  }
}

pub async fn srv<D: CertLoad>(
  shutdown_lock: Arc<tokio::sync::RwLock<()>>,
  conn_lock: Arc<RwLock<()>>,
  socket: UdpSocket,
  route: Arc<Route>,
  cert_loader: Arc<CertLoader<D>>,
) -> Result<()> {
  let tls = s2n_quic::provider::tls::s2n_tls::Server::builder()
    .with_client_hello_handler(Cert { cert_loader })?
    .build()?;

  let io = s2n_quic::provider::io::tokio::Builder::default()
    .with_rx_socket(socket)?
    .build()?;

  let mut server = Server::builder().with_tls(tls)?.with_io(io)?.start()?;

  loop {
    tokio::select! {
        conn = server.accept() => {
            if let Some(connection) = conn {
                let route = route.clone();
                let conn_lock = conn_lock.clone();
                tokio::spawn(
                    #[allow(clippy::await_holding_lock)]
                    async move {
                        let _guard = conn_lock.read();
                        if let Err(err) = handle_conn(connection, route).await {
                            log::warn!("h3 connection error: {err}");
                        }
                    }
                );
            } else {
                // Server closed
                break;
            }
        },
        _ = shutdown_lock.read() => {
            break;
        }
    }
  }

  log::info!("h3 server shut down");
  Ok(())
}

async fn handle_conn(conn: QuicConnection, route: Arc<Route>) -> Result<()> {
  let quic_conn = h3_quic::Connection::new(conn);
  let mut h3_conn = h3::server::Connection::new(quic_conn).await?;

  loop {
    match h3_conn.accept().await {
      Ok(Some(resolver)) => {
        let route = route.clone();
        tokio::spawn(async move {
          if let Ok((req, stream)) = resolver.resolve_request().await
            && let Err(e) = handle_req(req, stream, route).await
          {
            log::warn!("h3 request error: {e}");
          }
        });
      }
      Ok(None) => {
        break;
      }
      Err(_) => {
        break;
      }
    }
  }
  Ok(())
}

pub struct H3Body {
  stream: RequestStream<h3_quic::RecvStream, Bytes>,
}

impl H3Body {
  pub fn new(stream: RequestStream<h3_quic::RecvStream, Bytes>) -> Self {
    Self { stream }
  }
}

impl http_body::Body for H3Body {
  type Data = Bytes;
  type Error = Error;

  fn poll_frame(
    mut self: Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
    match self.stream.poll_recv_data(cx) {
      Poll::Ready(Ok(Some(data))) => {
        let bytes = data.chunk().to_vec();
        Poll::Ready(Some(Ok(Frame::data(Bytes::from(bytes)))))
      }
      Poll::Ready(Ok(None)) => Poll::Ready(None),
      Poll::Ready(Err(e)) => Poll::Ready(Some(Err(Error::H3Stream(e)))),
      Poll::Pending => Poll::Pending,
    }
  }
}

async fn handle_req(
  req: Request<()>,
  stream: RequestStream<h3_quic::BidiStream<Bytes>, Bytes>,
  route: Arc<Route>,
) -> Result<()> {
  let (parts, _) = req.into_parts();
  let (mut send_stream, recv_stream) = stream.split();
  let body = http_body_util::BodyStream::new(H3Body::new(recv_stream)).boxed();
  let req = Request::from_parts(parts, body);

  let resp = proxy(req, route).await;
  let (parts, body) = resp.into_parts();
  let resp = Response::from_parts(parts, ());

  send_stream.send_response(resp).await?;

  let mut body = body;
  while let Some(frame) = body.frame().await {
    if let Some(data) = frame?.data_ref() {
      send_stream.send_data(data.clone()).await?;
    }
  }
  send_stream.finish().await?;

  Ok(())
}
