#!/bin/bash

# setup.sh: 编译和安装 udpgrm 的脚本
#
# 这个脚本会从 GitHub 克隆 udpgrm 仓库,编译源代码,
# 并将必要的二进制文件 (`udpgrm`, `mmdecoy`, `udpgrm_activate.py`)
# 安装到 /usr/local/bin.

# 如果一个命令以非零状态退出,立即退出.
[ "$UID" -eq 0 ] || exec sudo "$0" "$@"
set -ex

# --- 配置 ---
INSTALL_PATH="/usr/local/bin"
REPO_URL="https://github.com/cloudflare/udpgrm.git"
REPO_DIR="udpgrm"

echo "安装路径: $INSTALL_PATH"

# --- 安装依赖 ---
echo "[INFO] 使用 apt-get 安装编译依赖..."
$SUDO apt-get update
$SUDO apt-get install -y git build-essential clang libelf-dev curl

# 安装 Rust 工具链
if ! command -v cargo &> /dev/null
then
    echo "[INFO] 安装 Rust 工具链..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # 将 cargo-env 添加到当前 shell 环境
    source "$HOME/.cargo/env"
else
    echo "[INFO] Rust 工具链已经安装."
fi


# --- 克隆仓库 ---
if [ -d "$REPO_DIR" ]; then
    echo "[INFO] '$REPO_DIR' 目录已存在,拉取最新变更..."
    cd "$REPO_DIR"
    git pull
    cd ..
else
    echo "[INFO] 克隆 udpgrm 仓库..."
    git clone --depth=1 "$REPO_URL"
fi

# --- 编译 ---
cd "$REPO_DIR"
echo "[INFO] 正在编译 udpgrm... (这可能需要一些时间)"
make

# --- 安装 ---
echo "[INFO] 安装二进制文件到 $INSTALL_PATH..."

TARGETS=("udpgrm" "mmdecoy" "tools/udpgrm_activate.py")

for target in "${TARGETS[@]}"; do
    if [ -f "$target" ]; then
        echo "       正在安装 $target..."
        $SUDO cp "$target" "$INSTALL_PATH/"
    else
        echo "[ERROR] 编译产物 '$target' 未找到. 终止安装."
        exit 1
    fi
done

# 赋予 python 脚本执行权限
chmod +x "$INSTALL_PATH/udpgrm_activate.py"

echo "以下文件已安装在 $INSTALL_PATH:"
ls -l "$INSTALL_PATH/udpgrm" "$INSTALL_PATH/mmdecoy" "$INSTALL_PATH/udpgrm_activate.py"
