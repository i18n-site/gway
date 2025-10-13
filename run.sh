#!/usr/bin/env bash

DIR=$(realpath $0) && DIR=${DIR%/*}
cd $DIR
set -ex

if ! [ -x "$(command -v systemfd)" ]; then
  cargo install systemfd
fi

systemfd -s tcp::0.0.0.0:8080 -s tcp::0.0.0.0:9083 -s udp::0.0.0.0:9083 -- cargo run -F cert_dir --example server
