#![cfg(unix)]

pub async fn signal() {
  use tokio::signal::unix::{SignalKind, signal};

  let mut term = signal(SignalKind::terminate()).unwrap();
  let mut hup = signal(SignalKind::hangup()).unwrap();

  let msg = tokio::select! {
      _ = tokio::signal::ctrl_c() => {
          "ctrl+c"
      },
      Some(_) = term.recv() => {
          "terminate"
      },
      Some(_) = hup.recv() => {
          "hangup"
      },
  };
  log::info!("\n收到 {} 信号，开始关闭服务", msg);
}
