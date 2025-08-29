# gway 优雅重启部署指南

本文档介绍如何使用 `systemd` 和 `udpgrm` 为 `gway` 服务实现优雅重启（零停机更新）。

## 核心思想

- **TCP 端口 (h1, h2)**: 使用 `systemd` 的 Socket Activation 功能。`gway.socket` 文件定义了需要监听的 TCP 端口。`systemd` 会在 `gway` 服务启动前创建好这些套接字，并将其传递给服务进程。
- **UDP 端口 (h3)**: 使用 Cloudflare 的 `udpgrm` 工具实现优雅重启。`gway.service` 文件中的 `ExecStartPre`指令会调用 `udpgrm_activate.py` 脚本来创建并注册 UDP 套接字。
- **优雅重启**: 当服务重启时 (`systemctl restart gway.service`), `systemd` 和 `udpgrm` 会确保旧的服务实例完成现有连接的处理，同时新的服务实例已经准备好接收新的连接，从而实现零停机。

## 依赖

1.  **`systemd`**: 现代 Linux 发行版通常都已内置。
2.  **`udpgrm`**: 需要预先安装并运行 `udpgrm` 守护进程。请参考 [udpgrm 官方文档](https://github.com/cloudflare/udpgrm) 进行安装。

## 文件说明

-   `gway.socket`: `systemd` 单元文件，定义了 h1 (`8080/tcp`) 和 h2 (`9083/tcp`) 的监听套接字。
-   `gway.service`: `systemd` 单元文件，定义了 `gway` 服务如何启动。它依赖 `gway.socket`，并使用 `udpgrm_activate.py` 来处理 h3 (`9083/udp`) 套接字。
-   `udpgrm_activate.py`: 从 `udpgrm` 项目中提取的辅助脚本，用于创建 UDP 套接字并将其注册到 `systemd` 的文件描述符存储中。
-   `deploy.sh`: 自动化部署脚本。

## 部署步骤

1.  确保 `udpgrm` 守护进程已经安装并正在运行。
2.  在 `gway` 项目根目录下，运行部署脚本：
    ```bash
    bash systemd/deploy.sh
    ```
    该脚本会自动完成以下工作：
    -   编译 `gway` 的 release 版本。
    -   将 `gway.service` 和 `gway.socket` 复制到 `/etc/systemd/system/`。
    -   将 `udpgrm_activate.py` 复制到 `/usr/local/bin/` 并赋予执行权限。
    -   重新加载 `systemd` 配置。
    -   启用并启动服务。

## 服务管理

部署完成后，你可以使用 `systemctl` 来管理 `gway` 服务。

-   **查看服务状态**:
    ```bash
    systemctl status gway.service
    ```

-   **启动服务**:
    ```bash
    sudo systemctl start gway.service
    ```

-   **停止服务**:
    ```bash
    sudo systemctl stop gway.service
    ```

-   **优雅重启服务 (零停机更新)**:
    ```bash
    sudo systemctl restart gway.service
    ```

-   **查看服务日志**:
    ```bash
    journalctl -u gway.service -f
    ```
