use std::{
  collections::HashMap,
  future::Future,
  net::{SocketAddr, TcpListener as StdTcpListener, UdpSocket},
  sync::Arc,
};

use faststr::FastStr;
use listenfd::ListenFd;
use parking_lot::RwLock;
use tokio::{
  net::TcpListener,
  task::{JoinHandle, JoinSet},
};

use crate::{CertLoad, CertLoader, Error, Result, Route};

pub mod h1;
pub mod h2;
pub mod h3;
pub mod s2n_quic;

/// 服务管理结构体
struct Srv {
  set: JoinSet<(FastStr, Result<()>)>,
  stop_handle: JoinHandle<()>,
}

impl Srv {
  fn new(stop_handle: JoinHandle<()>) -> Self {
    Self {
      set: JoinSet::new(),
      stop_handle,
    }
  }

  fn spawn<F>(&mut self, name: impl Into<FastStr>, fut: F)
  where
    F: Future<Output = Result<()>> + Send + 'static,
  {
    let name = name.into();
    self.set.spawn(async move { (name, fut.await) });
  }

  async fn join(&mut self) {
    while let Some(res) = self.set.join_next().await {
      self.stop_handle.abort();
      match res {
        Ok((name, Ok(_))) => log::info!("{name} 服务正常结束"),
        Ok((name, Err(e))) => log::warn!("{name} 服务因错误退出: {e}"),
        Err(e) => log::warn!("服务 panic: {e}"),
      }
    }
  }
}

pub async fn srv<D: CertLoad, F: Future<Output = ()> + Send + 'static>(
  route: Arc<Route>,
  cert_loader: Arc<CertLoader<D>>,
  graceful_shutdown: F,
  h1_addr: SocketAddr,
  h2_addr: SocketAddr,
  h3_addr: SocketAddr,
) -> Result<()> {
  let shutdown_lock = Arc::new(tokio::sync::RwLock::new(()));

  let stop_handle = tokio::spawn({
    let shutdown_lock = shutdown_lock.clone();
    async move {
      let _guard = shutdown_lock.write().await;
      graceful_shutdown.await
    }
  });

  let conn_lock = Arc::new(RwLock::new(()));

  let mut srv = Srv::new(stop_handle);

  let mut listenfd = ListenFd::from_env();

  let (mut tcp_listeners, mut udp_listeners) = if listenfd.len() > 0 {
    socket_from_listenfd(&mut listenfd)?
  } else {
    Default::default()
  };

  let h1_listener = get_or_create_tcp_listener(&mut tcp_listeners, h1_addr).await?;
  let h2_listener = get_or_create_tcp_listener(&mut tcp_listeners, h2_addr).await?;
  let h3_socket = get_or_create_udp_socket(&mut udp_listeners, h3_addr).await?;

  srv.spawn(
    "h1",
    h1::srv(
      shutdown_lock.clone(),
      conn_lock.clone(),
      h1_listener,
      route.clone(),
    ),
  );
  srv.spawn(
    "h2",
    h2::srv(
      shutdown_lock.clone(),
      conn_lock.clone(),
      h2_listener,
      route.clone(),
      cert_loader.clone(),
    ),
  );
  srv.spawn(
    "h3",
    h3::srv(
      shutdown_lock,
      conn_lock.clone(),
      h3_socket,
      route,
      cert_loader,
    ),
  );

  log::info!(
    "
h1 {h1_addr}
h2 {h2_addr}
h3 {h3_addr}"
  );

  srv.join().await;

  log::info!("等待所有连接关闭");
  {
    let _guard = conn_lock.write();
  }
  log::info!("所有连接都已关闭，退出进程");
  Ok(())
}

fn socket_from_listenfd(
  listenfd: &mut ListenFd,
) -> Result<(
  HashMap<SocketAddr, StdTcpListener>,
  HashMap<SocketAddr, UdpSocket>,
)> {
  let mut tcp_listeners = HashMap::new();
  let mut udp_sockets = HashMap::new();

  for i in 0..listenfd.len() {
    if let Ok(Some(l)) = listenfd.take_tcp_listener(i) {
      l.set_nonblocking(true)?;
      tcp_listeners.insert(l.local_addr()?, l);
    } else if let Ok(Some(l)) = listenfd.take_udp_socket(i) {
      l.set_nonblocking(true)?;
      udp_sockets.insert(l.local_addr()?, l);
    }
  }
  Ok((tcp_listeners, udp_sockets))
}

async fn get_or_create_tcp_listener(
  listeners: &mut HashMap<SocketAddr, StdTcpListener>,
  addr: SocketAddr,
) -> Result<TcpListener> {
  if let Some(listener) = listeners.remove(&addr) {
    log::info!("从环境变量获取TCP端口: {}", addr);
    return TcpListener::from_std(listener).map_err(Error::Io);
  }
  log::info!("手动创建TCP端口: {}", addr);
  TcpListener::bind(addr).await.map_err(Error::Io)
}

async fn get_or_create_udp_socket(
  sockets: &mut HashMap<SocketAddr, UdpSocket>,
  addr: SocketAddr,
) -> Result<UdpSocket> {
  let socket = if let Some(socket) = sockets.remove(&addr) {
    log::info!("从环境变量获取UDP端口: {}", addr);
    socket
  } else {
    log::info!("手动创建UDP端口: {}", addr);
    std::net::UdpSocket::bind(addr).map_err(Error::Io)?
  };
  Ok(socket)
}
