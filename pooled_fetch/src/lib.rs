use std::net::SocketAddr;

use http_body_util::Full;
use hyper::{
  Request, Response,
  body::{Bytes, Incoming},
  client::conn::http1::SendRequest,
};

mod body;
mod error;
mod http;
pub use body::{Body, POOL};
pub use error::{Error, Result};
pub use http::http;

pub type FullBytes = Full<Bytes>;
pub type Send = SendRequest<FullBytes>;

pub struct Sender {
  pub send: Send,
  pub conn: tokio::task::JoinHandle<()>,
}

pub struct Conn {
  peer_addr: SocketAddr,
  sender: Sender,
}

impl Conn {
  pub async fn send(&mut self, req: Request<FullBytes>) -> hyper::Result<Response<Incoming>> {
    self.sender.send.send_request(req).await
  }
}
