#!/usr/bin/env bash

DIR=$(realpath $0) && DIR=${DIR%/*/*}
cd $DIR

./sh/clippy.sh

git pull

if [ $# -eq 1 ]; then
  cd $1
fi

set -ex

rm -rf Cargo.lock

if ! [ -x "$(command -v cargo-v)" ]; then
  cargo install cargo-v
fi

cargo build

bun x mdt .
git add .
rm -rf Cargo.lock
touch Cargo.lock
cargo v patch -y

git describe --tags $(git rev-list --tags --max-count=1) | xargs git tag -d

rm Cargo.lock
git add -u
git commit -m. || true
git push
cargo publish --registry crates-io --allow-dirty || true
cd $DIR
git add -u
gme $(cargo metadata --format-version=1 --no-deps | jq '.packages[] | .name + ":" + .version' -r | grep "$name:") || true
