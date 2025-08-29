use std::{mem::ManuallyDrop, net::SocketAddr};

use hyper::{Request, Response, body::Incoming};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::{Body, Conn, FullBytes, POOL, Result, Sender};

pub async fn conn_new(addr: SocketAddr) -> Result<Conn> {
  let stream = TcpStream::connect(addr).await?;
  let peer_addr = stream.peer_addr()?;

  let io = TokioIo::new(stream);

  let (send, conn) = hyper::client::conn::http1::handshake(io).await?;

  let conn = tokio::task::spawn(async move {
    if let Err(err) = conn.await {
      log::info!("conn failed: {:?}", err);
    }
    let remove_key = {
      if let Some(conn_map) = POOL.get_mut(&addr) {
        conn_map.remove(&peer_addr);
        conn_map.is_empty()
      } else {
        false
      }
    };
    if remove_key {
      POOL.remove(&addr);
    }
  });

  Ok(Conn {
    peer_addr,
    sender: Sender { send, conn },
  })
}

fn res_body(res: Response<Incoming>, addr: SocketAddr, conn: Conn) -> Result<Response<Body>> {
  let res = res.map(|incoming| Body::new(incoming, addr, conn));
  Ok(res)
}

fn cached_conn(addr: SocketAddr) -> Option<Conn> {
  if let Some(sender_map) = POOL.get(&addr)
    && let Some(kv) = sender_map.pop_back()
  {
    return Some(Conn {
      peer_addr: *kv.key(),
      sender: ManuallyDrop::into_inner(unsafe { std::ptr::read(kv.value()) }),
    });
  }
  None
}

pub async fn http(addr: SocketAddr, req: Request<FullBytes>) -> Result<Response<Body>> {
  if let Some(mut conn) = cached_conn(addr) {
    match conn.send(req.clone()).await {
      Ok(res) => {
        return res_body(res, addr, conn);
      }
      Err(err) => {
        conn.abort();
        if !(err.is_canceled() || err.is_closed()) {
          return Err(err.into());
        }
      }
    }
  }
  let mut conn = conn_new(addr).await?;
  let res = conn.send(req).await?;
  res_body(res, addr, conn)
}
