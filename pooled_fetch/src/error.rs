use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
  #[error("io错误: {0}")]
  Io(#[from] std::io::Error),
  #[error("hyper错误: {0}")]
  Hyper(#[from] hyper::Error),
  #[error("地址解析错误: {0}")]
  AddrParse(#[from] std::net::AddrParseError),
}

pub type Result<T> = std::result::Result<T, Error>;
