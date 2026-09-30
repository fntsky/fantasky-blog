+++
date = '2025-12-23T22:06:10+08:00'
draft = true
title = 'Mini-Redis 学习分析（二）'
+++
#  Mini-Redis 学习分析（二）
### 0x01 连接上限的实现
在 `Listener` 结构中，`limit_connections` 变量用于限制同时连接的客户端数量。它是一个 `Arc<Semaphore>` 类型，通过信号量机制来控制并发连接数。

在Rust中,Arc（Atomic Reference Counted）是一种线程安全的引用计数智能指针,Semaphore:信号量是一种用于控制对共享资源访问的同步原语。

当一个新的客户端连接到来时，`Handler` 会尝试从 `Semaphore` 获取一个许可（permit）。如果当前连接数已达上限，新的连接请求将被阻塞，直到有连接断开并释放许可。

### 0x02 数据库互斥访问
在db.rs文件中通过使用`Mutex<State>`来确保对数据库状态的互斥访问。

采用std::sync::Mutex而非tokio::sync::Mutex的原因是因为数据库操作主要是CPU密集型任务，而不是I/O密集型任务。使用标准库的Mutex可以减少异步上下文切换的开销，提高性能。

### 0x03 关闭机制

