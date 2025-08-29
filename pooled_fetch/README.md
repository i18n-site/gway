# Pooled Fetch

[English README](#english-readme) | [中文说明](#中文说明)

`pooled-fetch` is a lightweight, asynchronous HTTP client connection pool for `hyper`. It's designed to enhance performance by reusing connections, thus avoiding the overhead of TCP and TLS handshakes for every request. The pool is resilient, automatically handling and replacing stale connections.

----

`pooled-fetch` 是一个基于 `hyper` 构建的轻量级、异步的 HTTP 客户端连接池。它旨在通过复用连接来提升性能，避免为每个请求都进行 TCP 和 TLS 握手的开销。该连接池具有弹性，能够自动处理和替换掉已失效的连接。

---

# English README

## Overview

This library provides a simple function, `http`, for making HTTP requests. It transparently manages a global pool of connections. When a request is made to a specific address, the library first attempts to retrieve a ready-to-use connection from the pool. If no connection is available, it establishes a new one and adds it to the pool for future use.

The magic lies in the custom `Body` wrapper. When the response body is fully consumed or dropped, the underlying connection is automatically returned to the pool, making it available for subsequent requests to the same destination. The pool is also resilient to stale connections, automatically replacing them when detected.

## Features

- **Connection Pooling**: Automatically reuses `hyper` client connections to reduce latency.
- **Resilient**: Automatically detects and replaces stale/broken connections.
- **Asynchronous**: Built on `tokio` and `hyper` for non-blocking I/O.
- **Thread-Safe**: Uses `dashmap` and `crossbeam-skiplist` to ensure safe concurrent access to the connection pool.
- **Automatic Lifecycle Management**: Connections are automatically returned to the pool upon response body completion and cleaned up if they break.

## How It Works

1.  **Request**: Call the `pooled_fetch::http(addr, request)` function.
2.  **Pool Check**: The library checks a global `DashMap` for a collection of connections to the target `SocketAddr`. It attempts to retrieve the most recently used connection from a `crossbeam-skiplist` map, which acts as a LIFO queue.
3.  **Connection Reuse & Resiliency**: If a cached connection is found, it's used to send the request. If the request fails because the connection is stale (e.g., closed by the server), the library automatically discards the stale connection and attempts to create a new one.
4.  **New Connection**: If no cached connection is available, a new one is established using `tokio::net::TcpStream` and `hyper`. A background task is spawned to monitor the connection; if it fails, it is automatically cleaned up from the pool.
5.  **Response and Return**: The response body is wrapped in a custom `Body` struct. Once the `Body` is dropped (i.e., the response is fully read or goes out of scope), its `Drop` implementation returns the healthy connection to the pool for the next request.

## Example Usage

```rust
use pooled_fetch::{http, FullBytes};
use hyper::{Request, body::Bytes, Uri};
use std::net::SocketAddr;
use std::str::FromStr;

async fn fetch(addr: SocketAddr, req: Request<FullBytes>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Making request to {}", addr);
    let res = http(addr, req).await? ;
    println!("Response Status: {}", res.status());
    // The connection is returned to the pool when `res` is dropped here.
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr_str = "127.0.0.1:8080"; // Replace with your target server
    let addr: SocketAddr = addr_str.parse()? ;

    let req = Request::builder()
        .uri(Uri::from_str("/")?)
        .body(FullBytes::new(Bytes::new()))? ;

    // First request: creates a new connection and adds it to the pool.
    fetch(addr, req.clone()).await? ;
    println!("First request done. Connection should be in the pool.");

    // Second request: reuses the connection from the pool.
    fetch(addr, req.clone()).await? ;
    println!("Second request done. Connection was reused.");

    Ok(())
}
```

---

# 中文说明

## 概述

`pooled-fetch` 是一个基于 `hyper` 构建的轻量级、异步的 HTTP 客户端连接池。它通过复用连接来提升性能，减少了为每个请求建立新的 TCP 连接和 HTTP 握手的开销。

本库提供了一个简单的 `http` 函数来发送 HTTP 请求，并透明地管理一个全局连接池。当向特定地址发出请求时，它会首先尝试从池中获取一个可用的连接。如果没有可用连接，它会建立一个新连接，并将其放入池中以备将来使用。

其核心机制在于自定义的 `Body` 封装类型。当响应体被完全消耗或被丢弃时，底层的连接会自动返回到池中，使其可以被后续发送到同一目标的请求复用。连接池对于失效连接也具有弹性，会在检测到时自动替换它们。

## 特性

- **连接池**: 自动复用 `hyper` 客户端连接，以降低延迟。
- **弹性设计**: 自动检测并替换失效或中断的连接。
- **异步**: 基于 `tokio` 和 `hyper` 构建，实现完全的非阻塞 I/O。
- **线程安全**: 使用 `dashmap` 和 `crossbeam-skiplist` 确保对连接池的并发访问是安全的。
- **自动生命周期管理**: 在响应体处理完毕后，连接会自动返回到池中；如果连接中断，则会被自动清理。

## 工作原理

1.  **发起请求**: 调用 `pooled_fetch::http(addr, request)` 函数。
2.  **检查池**: 库会检查一个全局的 `DashMap`，查找是否有到目标 `SocketAddr` 的连接集合。它会尝试从一个 `crossbeam-skiplist` 映射中获取最近使用的连接，该结构起到后进先出（LIFO）队列的作用。
3.  **复用与弹性**: 如果找到缓存的连接，就用它来发送请求。如果因为连接已失效（例如，被服务器关闭）而导致请求失败，库会自动丢弃该失效连接，并尝试创建新连接。
4.  **新建连接**: 如果没有可用的缓存连接，库会使用 `tokio::net::TcpStream` 和 `hyper` 建立一个新连接。同时会启动一个后台任务来监控此连接，一旦连接断开，它将被自动从池中清理。
5.  **响应与归还**: 响应体被封装在一个自定义的 `Body` 结构中。一旦 `Body` 被 `drop`（例如，响应被完全读取或超出作用域），其 `Drop` 实现会将健康的连接归还到池中，以供下一个请求使用。

## 使用示例

```rust
use pooled_fetch::{http, FullBytes};
use hyper::{Request, body::Bytes, Uri};
use std::net::SocketAddr;
use std::str::FromStr;

async fn fetch(addr: SocketAddr, req: Request<FullBytes>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("向 {} 发起请求", addr);
    let res = http(addr, req).await? ;
    println!("响应状态: {}", res.status());
    // `res` 在这里被 drop 时，连接会返回到池中。
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr_str = "127.0.0.1:8080"; // 替换为你的目标服务器
    let addr: SocketAddr = addr_str.parse()? ;

    let req = Request::builder()
        .uri(Uri::from_str("/")?)
        .body(FullBytes::new(Bytes::new()))? ;

    // 第一次请求：创建新连接并将其加入池中。
    fetch(addr, req.clone()).await? ;
    println!("第一次请求完成。连接应该已在池中。");

    // 第二次请求：从池中复用连接。
    fetch(addr, req.clone()).await? ;
    println!("第二次请求完成。连接已被复用。");

    Ok(())
}
```