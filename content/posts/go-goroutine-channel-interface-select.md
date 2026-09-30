+++
date = '2026-08-10T15:08:00+08:00'
draft = true
title = 'Go 并发四件套：goroutine、channel、interface、select'
tags = ['Go', '并发', 'etcd']
+++

# Go 并发四件套：goroutine、channel、interface、select


---

## 0. 先建立一个心智模型

用一句话记住四个概念的关系：

> **goroutine 是人，channel 是水管，select 是闸门，interface 是合同。**

- **goroutine**：并发的"人"，各自干活
- **channel**：人与人之间传递消息的管道
- **select**：一个人同时守着多根管道，谁先来消息就处理谁
- **interface**：约定"你会干什么"的合同，让不同的实现可以互换

Go 的并发哲学（CSP 模型）用一句话说就是：

> **不要通过共享内存来通信，而要通过通信来共享内存。**

---

## 1. goroutine：轻量级的"人"

### 1.1 是什么

在函数调用前加一个 `go` 关键字，这个函数就会在**独立的 goroutine** 里并发执行：

```go
func main() {
    go sayHello()          // 并发执行，不阻塞
    fmt.Println("main 继续干自己的事")
    time.Sleep(time.Millisecond) // 等一会儿，让 goroutine 跑完
}

func sayHello() {
    fmt.Println("你好，goroutine！")
}
```

### 1.2 为什么它很轻（速览）

| 对比项 | 操作系统线程 | goroutine |
|---|---|---|
| 初始栈大小 | 1~8 MB（创建时固定分配） | **2 KB（按需动态增长）** |
| 创建成本 | `clone()` 系统调用，内核参与，约 10µs 级 | 纯用户态分配结构体，约 100ns 级 |
| 切换成本 | 陷入内核，约 1~2µs，且缓存全凉 | 用户态寄存器保存/恢复，约几十 ns |
| 切换方式 | 内核抢占式调度（CFS） | 运行时协作 + 异步抢占 |
| 单进程数量级 | 几千个就吃力 | **几十万到百万没问题** |

差距来自底层实现：线程切换要陷入内核（保存寄存器、切页表、TLB 全失效），goroutine 切换只是用户态寄存器存档/恢复；线程栈 8MB 一次性锁死，goroutine 栈 2KB 起步、靠 copystack 动态增长；调度上内核 CFS vs 用户态 GMP（M 阻塞时 P 带队列转移，netpoller 让网络 IO 连 M 都不占）。

> 📌 **想深入底层？** 完整的机制拆解（内核切换五步、g 结构体与 gobuf、copystack、GMP vs CFS、work stealing、异步抢占）见姊妹篇：[《goroutine 为什么这么轻？线程与 goroutine 的底层对决》](/posts/go-goroutine-internals/)（草稿，发布后可见）

### 1.3 等待 goroutine 完成：sync.WaitGroup

`go` 关键字是"发射后不管"，要等它们干完，用 `WaitGroup`——**一个计数器**：

| 方法 | 作用 |
|---|---|
| `wg.Add(delta int)` | 计数器 +delta（通常在启动 goroutine 前调用） |
| `wg.Done()` | 计数器 -1（等价于 `Add(-1)`） |
| `wg.Wait()` | 阻塞，直到计数器归零 |

```go
func main() {
    var wg sync.WaitGroup
    for i := 1; i <= 5; i++ {
        wg.Add(1)
        go func(n int) {
            defer wg.Done()
            fmt.Printf("工人 %d 开工\n", n)
        }(i)
    }
    wg.Wait()
    fmt.Println("全部完成")
}
```

三条铁律：**Add 在 Wait 前**（别在 goroutine 里 Add）、**Done 用 defer 注册**（保证 panic 也减计数）、**WaitGroup 不可拷贝**（`go vet` 会报 copylocks）。

> 📌 **想深入？** 完整的 WaitGroup 指南（执行过程图解、底层原子计数器+信号量实现、三个易错点、Mutex/Once/Cond/errgroup 等同步原语清单）见姊妹篇：[《Go 并发收尾双雄：sync.WaitGroup 与 defer 完全指南》](/posts/go-sync-waitgroup-defer/)

### 1.4 defer：延迟执行（Go 的"收尾神器"）

`defer` 把一句话**推迟到函数返回前执行**——资源清理、解锁、日志收尾全靠它。

两条核心规则：
1. **执行时机**：`return` 先给返回值赋值，然后执行 defer，最后函数才真正返回
2. **LIFO 逆序**：多个 defer 按栈式后进先出执行

```go
func demo() {
    defer fmt.Println("③ 最后执行")
    defer fmt.Println("② 倒数第二")
    fmt.Println("① 先执行")
}
// 输出：① 先执行 → ② 倒数第二 → ③ 最后执行
```

最常用场景：`defer mu.Unlock()`（解锁）、`defer file.Close()`（关文件）、`defer wg.Done()`（WaitGroup）、`defer func(){ recover() }()`（兜底）、计时/日志收尾。

> 📌 **想深入？** 完整的 defer 指南（参数即时求值大坑、命名返回值被"偷改"、defer+recover 把 panic 转 error、deferproc/deferreturn 底层与 open-coded 优化、三个易错点）见姊妹篇：[《Go 并发收尾双雄：sync.WaitGroup 与 defer 完全指南》](/posts/go-sync-waitgroup-defer/)

### 1.5 两个大坑

**坑 1：goroutine 泄漏**。goroutine 阻塞在 channel 上永远等不到数据，就是泄漏（和内存泄漏一样危险）：

```go
ch := make(chan int)
go func() { <-ch }()   // 永远没人往 ch 发数据 → 这个 goroutine 永远挂起
```

**坑 2：panic 会杀死整个进程**。goroutine 里的 panic 无法被其他 goroutine 的 recover 捕获，一旦 panic 整个程序崩溃。所以 goroutine 里要有"护身符"：

```go
go func() {
    defer func() {
        if r := recover(); r != nil {
            fmt.Println("救回来了：", r)
        }
    }()
    panic("出事了")
}()
```

---

## 2. channel：goroutine 之间的"水管"

### 2.1 创建与类型

```go
ch1 := make(chan int)        // 无缓冲：同步通信（一手交钱一手交货）
ch2 := make(chan int, 10)    // 有缓冲：异步通信（信箱，最多放 10 条）
var ch3 chan int             // nil channel：既不能发也不能收
```

### 2.2 发送、接收、关闭

```go
ch := make(chan string)

// 发送（另一个 goroutine 里）
ch <- "hello"        // 数据流进管道

// 接收（本 goroutine 里）
msg := <-ch          // 数据流出管道
msg2, ok := <-ch     // ok=false 表示 channel 已关闭且没有剩余数据
close(ch)            // 关闭（只能发送方关）
```

### 2.3 无缓冲 vs 有缓冲（类比）

- **无缓冲 channel** = 两个人打电话：**必须同时在线**，发送方会阻塞直到接收方准备好
- **有缓冲 channel** = 邮箱：发送方把信塞进邮箱就走（邮箱没满的话），接收方有空再取

```go
func main() {
    ch := make(chan int, 2)   // 容量 2 的信箱
    ch <- 1                   // 不阻塞
    ch <- 2                   // 不阻塞
    // ch <- 3                // 信箱满了！会阻塞在这里
    fmt.Println(<-ch)         // 1
    fmt.Println(<-ch)         // 2
}
```

### 2.4 经典模式：生产者-消费者

```go
func producer(ch chan<- int) {          // chan<- 只写
    for i := 0; i < 5; i++ {
        ch <- i
        fmt.Println("生产：", i)
    }
    close(ch)                           // 生产完，关闭管道
}

func consumer(ch <-chan int) {          // <-chan 只读
    for v := range ch {                 // range 自动遍历到关闭
        fmt.Println("消费：", v)
    }
}

func main() {
    ch := make(chan int, 3)
    go producer(ch)
    consumer(ch)                        // main 里直接消费
}
```

注意 `chan<- int`（只写）和 `<-chan int`（只读）——**单向 channel** 是 Go 对"接口最小化"的表达：函数参数声明成单向，调用方就知道你只读不写（或只写不读），防止误用。

### 2.5 三个大坑

1. **向已关闭的 channel 发送 → panic**（`send on closed channel`）
2. **关闭已关闭的 channel → panic**（只能发送方关，且只关一次）
3. **死锁**：所有 goroutine 都在等 channel，谁也动不了 → 运行时直接报 `fatal error: all goroutines are asleep - deadlock!`

```go
// 死锁示例：main 自己等自己
func main() {
    ch := make(chan int)
    ch <- 1      // 无缓冲，没人收 → 阻塞 → 死锁
    <-ch
}
```

---

## 3. interface：Go 的"合同"

### 3.1 隐式实现（Duck Typing）

Go 的接口**不需要显式声明实现**——只要你的方法集匹配，你就是实现者：

```go
type Speaker interface {
    Speak() string
}

type Dog struct{}

func (d Dog) Speak() string { return "汪汪" }

type Cat struct{}

func (c Cat) Speak() string { return "喵喵" }

func announce(s Speaker) {   // 接受接口，不管具体是谁
    fmt.Println("它说：", s.Speak())
}

func main() {
    announce(Dog{})
    announce(Cat{})
}
```

类比：**接口是一份招聘合同**（"会 Speak 就行"），狗和猫都满足合同就能上岗。调用方只认合同，不认具体身份——这就是**依赖抽象而非实现**。

### 3.2 接口值：类型 + 值 的二元组

接口变量内部存了两样东西：**动态类型**和**动态值**。这带来一个经典陷阱——**存了 nil 指针的接口 ≠ nil**：

```go
type Speaker interface{ Speak() string }

type Dog struct{}

func (d *Dog) Speak() string { return "汪汪" }

func main() {
    var s Speaker
    var d *Dog = nil
    s = d              // 接口的动态类型是 *Dog，动态值是 nil
    if s == nil {      // ❌ 不会进这里！s 的类型不是 nil
        fmt.Println("s 是 nil")
    }
    // 正确判空方式：类型断言
    if _, ok := s.(*Dog); !ok || s == nil {
        fmt.Println("s 实际是空的 *Dog")
    }
}
```

### 3.3 空接口 `any`：万能容器

```go
var anything any = 42
anything = "字符串"
anything = []int{1, 2, 3}

// 取回真实类型：类型断言
if num, ok := anything.(int); ok {
    fmt.Println("是数字：", num)
}

// 或 type switch
switch v := anything.(type) {
case int:
    fmt.Println("int:", v)
case string:
    fmt.Println("string:", v)
default:
    fmt.Println("未知类型")
}
```

### 3.4 实战：etcd 里的 interface（划重点！）

etcd 的 raft 共识库定义了一个 `Storage` 接口，**只要求"你能存日志、能取日志"**：

```go
// raft-src/storage.go（go.etcd.io/raft 独立库）
type Storage interface {
    InitialState() (pb.HardState, pb.ConfState, error)
    Entries(lo, hi, maxSize uint64) ([]pb.Entry, error)
    Term(i uint64) (uint64, error)
    LastIndex() (uint64, error)
    FirstIndex() (uint64, error)
    Snapshot() (pb.Snapshot, error)
}
```

而 etcd 用 **WAL（预写日志）** 实现了这个接口。raft 算法本身**完全不关心**你的日志存在内存、磁盘还是云上——它只认这个合同。这就是"**算法与存储解耦**"：想换存储实现，只要满足合同就行，raft 一行都不用改。

> 这就是接口最大的价值：**让系统可以拆分、可以替换、可以测试**（测试时可以传入一个假的 Storage 实现）。

### 3.5 两个设计建议

1. **接口要小**：一个方法最好（`io.Reader` 就只有一个 `Read`）
2. **接口定义在使用方**，而不是实现方（Go 惯例：实现方不需要声明"我实现了 XX"）

---

## 4. select：多路复用"闸门"

### 4.1 语法

`select` 同时监听多个 channel，**哪个先准备好就执行哪个**；多个同时准备好则**随机**选一个：

```go
func main() {
    ch1 := make(chan string)
    ch2 := make(chan string)

    go func() { time.Sleep(100 * time.Millisecond); ch1 <- "一号消息" }()
    go func() { time.Sleep(200 * time.Millisecond); ch2 <- "二号消息" }()

    for i := 0; i < 2; i++ {
        select {
        case msg := <-ch1:
            fmt.Println("收到：", msg)
        case msg := <-ch2:
            fmt.Println("收到：", msg)
        }
    }
}
```

### 4.2 三大实战模式

**模式一：超时控制**

```go
ch := make(chan int)
go func() { time.Sleep(5 * time.Second); ch <- 1 }()

select {
case v := <-ch:
    fmt.Println("拿到结果：", v)
case <-time.After(1 * time.Second):
    fmt.Println("超时了，不等了")
}
```

**模式二：for + select 主循环（etcd 同款！）**

生产级服务最常见的骨架。这是 etcd 服务端主循环的真实简化版（`server/etcdserver/server.go` 第 841 行）：

```go
func (s *EtcdServer) run() {
    for {
        select {
        case ap := <-s.r.apply():        // ① raft 提交的新日志来了
            // 处理日志（写数据库）
        case leases := <-expiredLeaseC:  // ② 有过期租约
            // 撤销过期租约
        case err := <-s.errorc:          // ③ 出错了
            return
        case <-s.stop:                   // ④ 收到停机信号
            return
        }
    }
}
```

一个 `for + select` 同时管理四路信号——**这就是"事件循环"**，也是绝大多数服务器/中间件的核心骨架。读懂这段，你就能读懂 etcd、Kafka、Redis 的主循环。

**模式三：nil channel 禁用分支**

`nil channel` 在 select 里**永远不会就绪**，利用这一点可以动态开关某个分支：

```go
var dataCh chan int      // 初始为 nil，禁用
enable := false

// 某个时刻启用：
if enable {
    dataCh = make(chan int)
}

select {
case v := <-dataCh:      // dataCh 为 nil 时，这个分支永远不会被选中
    fmt.Println(v)
default:
    fmt.Println("数据通道未启用")
}
```

### 4.3 三个坑

1. **空 `select {}`**：永久阻塞（可用于让 main 不退出，但一般用 `select {}` 要小心）
2. **随机选择**：不要依赖 select 的分支顺序
3. **已关闭的 channel 永远就绪**：`<-closedCh` 立即返回零值，select 会一直命中它——**收到关闭信号后要把 channel 置为 nil** 来禁用分支

---

## 5. 综合实战：用四件套写一个"迷你 watch 服务"

etcd 的 watch 功能：客户端订阅某个 key，key 一变就收到通知。我们用刚学的四件套模拟它：

```go
package main

import (
	"fmt"
	"time"
)

// ① interface：事件源合同（etcd 里这是 watchableStore）
type EventSource interface {
	Watch(key string) (<-chan string, func()) // 返回事件管道 + 取消函数
}

// ② 实现：内存版事件源
type memorySource struct {
	mu      sync.Mutex
	watchers map[string][]chan string
}

func newMemorySource() *memorySource {
	return &memorySource{watchers: make(map[string][]chan string)}
}

func (m *memorySource) Watch(key string) (<-chan string, func()) {
	m.mu.Lock()
	defer m.mu.Unlock()
	ch := make(chan string, 10)
	m.watchers[key] = append(m.watchers[key], ch)
	return ch, func() { /* 取消逻辑省略 */ }
}

func (m *memorySource) Publish(key, value string) {
	m.mu.Lock()
	chs := append([]chan string(nil), m.watchers[key]...)
	m.mu.Unlock()
	for _, ch := range chs { // ③ goroutine + channel 广播
		ch <- fmt.Sprintf("%s = %s", key, value)
	}
}

// ④ 订阅者：for + select 同时听事件和超时
func subscriber(name string, ch <-chan string, cancel func()) {
	for {
		select {
		case ev := <-ch:
			fmt.Printf("[%s] 收到事件：%s\n", name, ev)
		case <-time.After(3 * time.Second):
			fmt.Printf("[%s] 3 秒没动静，退出\n", name)
			cancel()
			return
		}
	}
}

func main() {
	src := newMemorySource()

	// 两个订阅者
	chA, cancelA := src.Watch("greeting")
	go subscriber("订阅者A", chA, cancelA)

	// 主 goroutine 模拟 etcd 主循环：发布事件
	time.Sleep(200 * time.Millisecond)
	src.Publish("greeting", "你好")
	src.Publish("greeting", "世界")

	time.Sleep(4 * time.Second) // 等订阅者超时退出
	fmt.Println("迷你 watch 服务演示结束")
}
```

运行输出：

```
[订阅者A] 收到事件：greeting = 你好
[订阅者A] 收到事件：greeting = 世界
[订阅者A] 3 秒没动静，退出
迷你 watch 服务演示结束
```

（代码需要导入 `sync` 包，完整可运行版本见文末附录。）

这个例子里四件套全齐了：
- **interface** 定义了事件源的合同，内存版只是其中一个实现
- **channel** 把事件从发布者送到订阅者
- **goroutine** 让订阅者独立运行
- **select** 让订阅者同时监听"事件"和"超时"两路信号

---

## 6. 总结

| 概念 | 一句话 | 记不住时想 |
|---|---|---|
| goroutine | `go` 关键字启动的轻量并发单元 | 跑腿的人 |
| channel | 类型安全的通信管道，`<-` 收发 | 水管 |
| interface | 隐式实现的方法集合同 | 招聘合同 |
| select | 多 channel 多路复用，谁就绪处理谁 | 闸门 |

**学完去 etcd 源码里找它们**（对照 `E:/workspace/etcd-study` 的 analysis 笔记）：

- `server/etcdserver/server.go:841` — `for + select` 主循环
- `server/etcdserver/raft.go:174` — raftNode 的 `select` 循环（ticker / Ready / stopped）
- `raft-src/storage.go` — `Storage` 接口（WAL 实现它）
- `server/storage/mvcc/watchable_store.go` — watch 的 channel 广播

---

## 附录：综合示例完整代码

```go
package main

import (
	"fmt"
	"sync"
	"time"
)

type EventSource interface {
	Watch(key string) (<-chan string, func())
}

type memorySource struct {
	mu       sync.Mutex
	watchers map[string][]chan string
}

func newMemorySource() *memorySource {
	return &memorySource{watchers: make(map[string][]chan string)}
}

func (m *memorySource) Watch(key string) (<-chan string, func()) {
	m.mu.Lock()
	defer m.mu.Unlock()
	ch := make(chan string, 10)
	m.watchers[key] = append(m.watchers[key], ch)
	return ch, func() {}
}

func (m *memorySource) Publish(key, value string) {
	m.mu.Lock()
	chs := append([]chan string(nil), m.watchers[key]...)
	m.mu.Unlock()
	for _, ch := range chs {
		ch <- fmt.Sprintf("%s = %s", key, value)
	}
}

func subscriber(name string, ch <-chan string, cancel func()) {
	for {
		select {
		case ev := <-ch:
			fmt.Printf("[%s] 收到事件：%s\n", name, ev)
		case <-time.After(3 * time.Second):
			fmt.Printf("[%s] 3 秒没动静，退出\n", name)
			cancel()
			return
		}
	}
}

func main() {
	src := newMemorySource()
	chA, cancelA := src.Watch("greeting")
	go subscriber("订阅者A", chA, cancelA)

	time.Sleep(200 * time.Millisecond)
	src.Publish("greeting", "你好")
	src.Publish("greeting", "世界")

	time.Sleep(4 * time.Second)
	fmt.Println("迷你 watch 服务演示结束")
}
```
