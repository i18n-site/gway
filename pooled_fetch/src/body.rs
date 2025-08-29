use std::{mem::ManuallyDrop, net::SocketAddr};

use crossbeam_skiplist::SkipMap;
use dashmap::DashMap;
use hyper::body::{Bytes, Incoming};

use crate::{Conn, Sender};

#[static_init::dynamic]
pub static POOL: DashMap<SocketAddr, SkipMap<SocketAddr, ManuallyDrop<Sender>>> = DashMap::new();

pub struct Body {
  incoming: Incoming,
  sender: ManuallyDrop<Sender>,
  addr: SocketAddr,
  peer_addr: SocketAddr,
}

impl Body {
  pub fn new(incoming: Incoming, addr: SocketAddr, conn: Conn) -> Self {
    Self {
      incoming,
      addr,
      peer_addr: conn.peer_addr,
      sender: ManuallyDrop::new(conn.sender),
    }
  }
}

impl Drop for Body {
  fn drop(&mut self) {
    let sender = unsafe { ManuallyDrop::take(&mut self.sender) };
    POOL
      .entry(self.addr)
      .or_default()
      .insert(self.peer_addr, ManuallyDrop::new(sender));
  }
}

impl http_body::Body for Body {
  type Data = Bytes;
  type Error = hyper::Error;

  fn poll_frame(
    self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<std::result::Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
    http_body::Body::poll_frame(std::pin::Pin::new(&mut self.get_mut().incoming), cx)
  }
}
