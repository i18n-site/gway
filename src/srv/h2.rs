use std::sync::Arc;

use hyper::{Request, body::Incoming, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use parking_lot::RwLock;
use tokio::net::TcpListener;
use tokio_rustls::rustls::{self, ServerConfig};

use crate::{
  Result, Route,
  cert_loader::{CertLoad, CertLoader},
  proxy,
};

pub async fn srv<D: CertLoad>(
  shutdown_lock: Arc<tokio::sync::RwLock<()>>,
  conn_lock: Arc<RwLock<()>>,
  listener: TcpListener,
  route: Arc<Route>,
  cert_loader: Arc<CertLoader<D>>,
) -> Result<()> {
  // let listener = TcpListener::bind(addr).await?;

  loop {
    tokio::select! {
        res = listener.accept() => {
            let (stream, _remote_addr) = match res {
                Ok(val) => val,
                Err(e) => {
                    log::warn!("h2 accept error: {e}");
                    continue;
                }
            };
            let route = route.clone();
            let cert_loader = cert_loader.clone();
            let conn_lock = conn_lock.clone();

            tokio::spawn(
            #[allow(clippy::await_holding_lock)]
            async move {
                let _guard = conn_lock.read();
                let acceptor =
                    tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream).await;

                let stream = match acceptor {
                    Ok(start_handshake) => {
                        let client_hello = start_handshake.client_hello();
                        if let Some(server_name) = client_hello.server_name()
                            && !server_name.is_empty()
                        {
                            let cert = match cert_loader.get(server_name).await {
                                Ok(cert) => cert,
                                Err(err) => {
                                    log::warn!("h2 tls: cert get error: {err}");
                                    return;
                                }
                            };

                            let mut tls_config = match ServerConfig::builder()
                                .with_no_client_auth()
                                .with_single_cert(cert.rustls.fullchain.clone(), cert.rustls.key.clone_key())
                            {
                                Ok(config) => config,
                                Err(err) => {
                                    log::warn!("h2 tls: server config error: {err}");
                                    return;
                                }
                            };

                            tls_config.alpn_protocols = vec![b"h2".to_vec()];
                            match start_handshake.into_stream(Arc::new(tls_config)).await {
                                Ok(stream) => stream,
                                Err(err) => {
                                    log::warn!("h2 tls: handshake error: {err}");
                                    return;
                                }
                            }
                        } else {
                            log::warn!("h2 tls: server name is empty");
                            return;
                        }
                    }
                    Err(err) => {
                        log::warn!("h2 tls: lazy acceptor error: {err}");
                        return;
                    }
                };

                let io = TokioIo::new(stream);
                let service = service_fn(move |req: Request<Incoming>| {
                    let route = route.clone();
                    async move { Ok::<_, hyper::Error>(proxy(req, route).await) }
                });

                let conn_builder = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
                if let Err(err) = conn_builder.serve_connection(io, service).await {
                    log::warn!("h2 error: {err}");
                }
            });
        },
        _ = shutdown_lock.read() => {
            break;
        }
    }
  }

  log::info!("h2 server shut down");
  Ok(())
}
