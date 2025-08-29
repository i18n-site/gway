#!/usr/bin/env bash

# 该脚本用于部署 gway 服务到 systemd

set -ex

DIR=$(realpath $0) && DIR=${DIR%/*}
PROJECT_ROOT=${DIR%/*}

echo "编译 gway release 版本..."
cargo build --release --example server

echo "拷贝 systemd 单元文件..."
sudo cp "$DIR/gway.service" /etc/systemd/system/
sudo cp "$DIR/gway.socket" /etc/systemd/system/

echo "拷贝 udpgrm_activate.py 到 /usr/local/bin/ ..."
sudo cp "$DIR/udpgrm_activate.py" /usr/local/bin/
sudo chmod +x /usr/local/bin/udpgrm_activate.py

echo "重新加载 systemd 配置..."
sudo systemctl daemon-reload

echo "启用并启动 gway.socket..."
sudo systemctl enable --now gway.socket

echo "重启 gway.service..."
sudo systemctl restart gway.service

echo "部署完成！"
echo "你可以使用 'systemctl status gway.service' 查看服务状态。"
echo "优雅重启请使用 'systemctl restart gway.service'。"
