use rand::Rng;

pub fn randstr(size: usize) -> String {
  const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::rng();
  (0..size)
    .map(|_| {
      let idx = rng.random_range(0..CHARSET.len());
      CHARSET[idx] as char
    })
    .collect()
}
