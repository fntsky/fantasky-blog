      +++
date = '2026-08-10T15:26:00+08:00'
draft = true
title = 'gRPC 与 Protobuf：像调本地函数一样调远程服务'
tags = ['gRPC', 'Protobuf', 'Go', 'etcd']
+++

# gRPC 与 Protobuf：像调本地函数一样调远程服务

> 这是 Go 并发系列（[《Go 并发四件套》](/posts/go-goroutine-channel-interface-select/)）的配套篇，也是 etcd 学习链路 L0 的前置知识。
> 为什么必须懂它？**etcd 对外暴露的 API 全部是 gRPC**——`api/etcdserverpb/rpc.proto` 一份文件定义六大服务，
> 客户端（etcdctl、Kubernetes）通过自动生成的 stub 调用，就像调用本地函数一样。
> 这篇博客带你亲手走一遍：**写 .proto → 生成代码 → 跑起服务 → 客户端调用**。

---

## 0. 先建立一个心智模型

三个角色，一句话：

> **.proto 文件是"合同模板"，protoc 是"合同翻译机"，stub 是"远程代理人"。**

- **.proto**：用中立语法描述"消息长什么样、提供哪些服务"（不绑定任何语言）
- **protoc**：把 .proto 翻译成各种语言的代码（Go / Python / Java / C++…）
- **stub（存根）**：生成的客户端代码，让你**像调本地函数一样调远程服务**——它负责把参数打包、发网络、解包返回值

---

## 1. Protobuf：带 schema 的二进制序列化

### 1.1 为什么需要它（JSON 的痛点）

跨服务传数据，最朴素的做法是 JSON：`{"name": "huan", "age": 18}`。它的痛点：

| 问题 | 说明 |
|---|---|
| **大** | 字段名重复出现（`"name"` 写了 1 遍，1000 条记录写 1000 遍），网络和存储都浪费 |
| **慢** | 文本解析（词法+语法分析）比二进制解析慢一个数量级 |
| **没约束** | JSON 没有 schema，字段拼错、类型传错只有运行时才知道 |

Protobuf 的解法：**定义一份 schema（.proto），把数据编码成紧凑的二进制**。

### 1.2 .proto 文件长什么样（proto3 语法）

```proto
syntax = "proto3";                 // 语法版本

package hello;                     // 命名空间（生成代码的包名）

message HelloRequest {             // 消息 = 一个数据结构
  string name = 1;                 // 字段：类型 名字 = 编号
}

message HelloReply {
  string message = 1;
}

service Greeter {                  // 服务 = 一组 RPC 方法
  rpc SayHello(HelloRequest) returns (HelloReply);
}
```

四个必须懂的概念：

1. **字段编号（= 1）**：线上传输时**只传编号不传名字**——这是 Protobuf 小的根本原因
2. **repeated**：数组（`repeated string tags = 2;`）
3. **enum**：枚举类型
4. **service**：RPC 服务定义（第 2 节详讲）

### 1.3 编译生成代码：stub 的来历

`protoc` 把 .proto 编译成目标语言代码：

```bash
# 生成 Python 代码（grpcio-tools 自带 protoc）
python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. hello.proto
```

生成两个文件：

```
hello_pb2.py           # 消息的序列化/反序列化代码（HelloRequest、HelloReply）
hello_pb2_grpc.py      # 服务的 stub 代码（GreeterStub + GreeterServicer）
```

**同样的 .proto 可以生成任意语言的代码**——这就是"合同"的价值：Go 服务端 + Python 客户端 + Java 客户端，各看各的合同，互不关心对方用什么语言。

### 1.4 字段编号的玄机（兼容性）

编号是字段**线上的身份**，它决定了新老版本能否互通：

| 操作 | 安全吗 | 原因 |
|---|---|---|
| 改字段名 | ✅ 安全 | 线上只认编号 |
| **改字段编号** | ❌ 致命 | 老版本读到新含义的数据，直接错乱 |
| 加新字段 | ✅ 安全 | 老版本遇到未知编号会跳过 |
| 删字段 | ⚠️ 保留编号 | 删除后要"占坑"（`reserved 2;`），防止未来复用编号导致新旧混读 |

**教训**：编号是永久的身份证号，一旦发布，永不更改、永不复用。

### 1.5 JSON vs Protobuf 对比

| | JSON | Protobuf |
|---|---|---|
| 格式 | 文本，人类可读 | 二进制，不可读（但可转 JSON 调试） |
| 体积 | 大（字段名重复） | **小 3~10 倍** |
| 解析速度 | 慢（文本解析） | **快一个数量级** |
| Schema | 无（运行时才知道对错） | 有（编译期就能校验） |
| 调试友好 | ✅ 直接看 | ❌ 需要工具（`protoc --decode`） |
| 适用场景 | 浏览器 API、日志、配置文件 | **服务间高性能通信** |

---

## 2. gRPC：基于 HTTP/2 的 RPC 框架

### 2.1 从本地调用到远程调用

你写 `client.SayHello(req)`，背后发生了什么？

```
你的代码 ──调用──▶ stub.SayHello()
                       │ ① 把 req 序列化成二进制（Protobuf）
                       │ ② 通过 HTTP/2 发送给服务器
                       ▼
                  服务器  GreeterServicer.SayHello()
                       │ ③ 反序列化，执行你的业务逻辑
                       │ ④ 把返回值序列化发回
                       ▼
你的代码 ◀──返回── stub 收到响应，反序列化成 HelloReply
```

**你感知不到网络的存在**——这就是 RPC 的魅力：分布式系统编程得像单机一样。

### 2.2 为什么用 HTTP/2 而不是 HTTP/1.1

| 特性 | 意义 |
|---|---|
| 多路复用 | 一个连接同时跑多个请求，不用排队（HTTP/1.1 队头阻塞） |
| 二进制帧 | 与 Protobuf 二进制天然契合 |
| 双向流 | 服务器可以主动持续推送（watch 的底层！） |
| 头部压缩 | 请求头小（HPACK） |

### 2.3 四种调用模式（对应不同的通信需求）

| 模式 | 语法 | 场景 | etcd 里的例子 |
|---|---|---|---|
| **Unary**（一元） | `rpc F(A) returns (B)` | 一问一答 | `KV.Put`、`KV.Range` |
| **Server streaming**（服务端流） | `returns (stream B)` | 服务端持续推送 | `Maintenance.Snapshot`（备份数据流） |
| **Client streaming**（客户端流） | `rpc F(stream A) returns (B)` | 客户端持续上传 | 批量写 |
| **Bidirectional streaming**（双向流） | `rpc F(stream A) returns (stream B)` | 双方持续通信 | **`Watch.Watch`**（监听 key 变化） |

### 2.4 gRPC vs REST 对比

| | REST | gRPC |
|---|---|---|
| 协议 | HTTP/1.1 + JSON | HTTP/2 + Protobuf |
| 接口定义 | 无（文档、OpenAPI） | **.proto 强制 schema** |
| 方法语义 | GET/POST/PUT/DELETE + URL | 任意命名的方法 |
| 流式 | 难（SSE 等补丁） | **一等公民** |
| 适合 | 浏览器/公开 API | **内部服务间通信** |

---

## 3. 实战：从零写一个 gRPC 服务

> 全程在 WSL（Ubuntu 24.04）实测，工具：`grpcio-tools`（Python 版 protoc，免 root）。

### 3.1 装工具链

```bash
python3 -m venv ~/grpc-venv
~/grpc-venv/bin/pip install grpcio-tools
```

### 3.2 定义 hello.proto

```proto
syntax = "proto3";

package hello;

message HelloRequest {
  string name = 1;
}

message HelloReply {
  string message = 1;
}

service Greeter {
  // 一元调用：一问一答
  rpc SayHello(HelloRequest) returns (HelloReply);
  // 服务端流：一次请求，三条问候
  rpc SayHelloStream(HelloRequest) returns (stream HelloReply);
}
```

### 3.3 生成 stub

```bash
~/grpc-venv/bin/python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. hello.proto
ls hello_pb2*.py
# hello_pb2.py        ← 消息代码（序列化）
# hello_pb2_grpc.py   ← 服务代码（stub + servicer 基类）
```

### 3.4 写服务端 server.py

```python
import time
from concurrent.futures import ThreadPoolExecutor
import grpc
import hello_pb2
import hello_pb2_grpc

class Greeter(hello_pb2_grpc.GreeterServicer):
    def SayHello(self, request, context):
        # 一元模式：收到请求，返回响应
        return hello_pb2.HelloReply(message=f"你好，{request.name}！")

    def SayHelloStream(self, request, context):
        # 服务端流模式：yield 持续推送
        for i in range(3):
            yield hello_pb2.HelloReply(message=f"第 {i+1} 条问候：你好，{request.name}！")
            time.sleep(0.5)

def serve():
    server = grpc.server(ThreadPoolExecutor(max_workers=4))
    hello_pb2_grpc.add_GreeterServicer_to_server(Greeter(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    print("gRPC server listening on :50051")
    server.wait_for_termination()

if __name__ == "__main__":
    serve()
```

### 3.5 写客户端 client.py

```python
import grpc
import hello_pb2
import hello_pb2_grpc

def main():
    with grpc.insecure_channel("localhost:50051") as ch:
        stub = hello_pb2_grpc.GreeterStub(ch)   # ← 这就是"远程代理人"
        resp = stub.SayHello(hello_pb2.HelloRequest(name="大管家"))
        print("一元响应：", resp.message)
        for r in stub.SayHelloStream(hello_pb2.HelloRequest(name="etcd")):
            print("流式响应：", r.message)

if __name__ == "__main__":
    main()
```

### 3.6 运行输出（实机验证）

```bash
$ python server.py &        # 终端 1：起服务
gRPC server listening on :50051

$ python client.py          # 终端 2：调客户端
一元响应： 你好，大管家！
流式响应： 第 1 条问候：你好，etcd！
流式响应： 第 2 条问候：你好，etcd！
流式响应： 第 3 条问候：你好，etcd！
```

**注意**：客户端写起来就像调用本地函数——`stub.SayHello(req)` 一行，序列化、网络传输、反序列化全部被 stub 藏起来了。这就是 RPC 的魔法。

---

## 4. etcd 里的 gRPC/Protobuf（划重点）

### 4.1 一份 .proto 定义六大服务

etcd 的 `api/etcdserverpb/rpc.proto`（本仓库已克隆，可对照阅读）：

| 服务 | 主要 RPC | 一句话职责 | rpc.proto 行号 |
|---|---|---|---|
| **KV** | Range / Put / DeleteRange / Txn / Compact | 键值读写、事务、压缩 | :33 |
| **Watch** | Watch（双向流） | 订阅 key 变化 | :92 |
| **Lease** | LeaseGrant / KeepAlive / Revoke | 租约 TTL 管理 | :106 |
| **Cluster** | MemberAdd / Remove / List | 集群成员管理 | :163 |
| **Maintenance** | Snapshot / Defragment / Hash / Alarm | 运维管理 | :205 |
| **Auth** | AuthEnable / UserAdd / RoleGrant | 认证与 RBAC | :279 |

### 4.2 Watch 为什么是双向流

`rpc Watch(stream WatchRequest) returns (stream WatchResponse)`——客户端持续发"我要监听 key X"，服务端持续回"key 变了！"。双向流让**服务端可以主动推送**，这是 HTTP/1.1 时代做不到的。

### 4.3 clientv3 就是 stub 的使用方

etcd 客户端库 `client/v3` 本质上就是 `rpc.proto` 生成的 stub 的封装：`clientv3.New(...)` 创建连接，`cli.Put(ctx, key, value)` 调用的就是 `KV.Put` 这个 RPC。**Kubernetes 里所有 `kubectl get` 背后，都是一次次 gRPC 调用。**

---

## 5. 总结

| 概念 | 一句话 |
|---|---|
| Protobuf | 带 schema 的二进制序列化，字段编号是永久的线上身份 |
| protoc | 把 .proto 翻译成任意语言代码的"合同翻译机" |
| gRPC | 基于 HTTP/2 的 RPC 框架，四种流式模式 |
| stub | 远程代理人，让分布式编程像本地调用 |

**下一步**：打开 etcd 仓库的 `api/etcdserverpb/rpc.proto`，把六大服务逐个看一遍；再看 `server/etcdserver/api/v3rpc/key.go`，看 `kvServer.Put` 怎么处理收到的请求——你就把"合同 → 实现"的链路看全了。
