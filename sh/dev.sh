#!/usr/bin/env bash

DIR=$(realpath $0) && DIR=${DIR%/*}
cd $DIR

source ./pid.sh

set -ex


systemfd -s tcp::0.0.0.0:8080 -s tcp::0.0.0.0:9083 -s udp::0.0.0.0:9083 -- cargo watch -x 'run -F cert_dir --example server'
