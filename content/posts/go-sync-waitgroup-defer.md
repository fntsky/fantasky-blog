+++
date = '2026-08-10T15:20:00+08:00'
draft = true
title = 'Go 并发收尾双雄：sync.WaitGroup 与 defer 完全指南'
tags = ['Go', '并发', 'sync']
+++

# Go 并发收尾双雄：sync.WaitGroup 与 defer 完全指南

> 这是 [《Go 并发四件套：goroutine、channel、interface、select》](/posts/go-goroutine-channel-interface-select/) 的姊妹篇。
> 主文章讲"怎么把 goroutine 发出去"，这篇讲**怎么收尾**：等它们干完（WaitGroup）、保证资源一定释放（defer）。
> 这两样是 Go 并发代码里出现频率最高的"配套设施"，值得单独一篇讲透。

---

## 1. sync.WaitGroup：等所有 goroutine 干完

`go` 关键字是"发射后不管"，要等它们干完，用 `WaitGroup`——**一个计数器**：

| 方法 | 作用 |
|---|---|
| `wg.Add(delta int)` | 计数器 +delta（通常在启动 goroutine 前调用） |
| `wg.Done()` | 计数器 -1（等价于 `Add(-1)`，通常在 goroutine 结束时调用） |
| `wg.Wait()` | 阻塞，直到计数器归零 |

```go
func main() {
    var wg sync.WaitGroup
    for i := 1; i <= 5; i++ {
        wg.Add(1)                          // ① 计数器 +1
        go func(n int) {
            defer wg.Done()                // ③ 干完活，计数器 -1
            fmt.Printf("工人 %d 开工\n", n)
        }(i)
    }
    wg.Wait()                              // ② 阻塞等计数器归零
    fmt.Println("全部完成")
}
```

### 1.1 执行过程（计数器视角）

```
Add(1) → 计数器 = 1 → goroutine 开工 → Done() → 计数器 = 0
Add(1) → 计数器 = 2 → goroutine 开工 → Done() → 计数器 = 1
...
计数器归零的那一刻，Wait() 解除阻塞，main 继续
```

### 1.2 标准使用模式（三条铁律）

1. **`Add` 必须在 `Wait` 之前**（或与启动 goroutine 同步），不要在 goroutine 内部 `Add`
2. **`Done` 用 `defer` 注册**——保证 goroutine 无论正常结束还是 panic，计数器都会被减（这就是 defer 的用武之地，见下文第 2 节）
3. **`WaitGroup` 不能被拷贝**（拷贝后计数器不共享，`go vet` 会报 copylocks 警告）

### 1.3 底层实现（一句话版）

`WaitGroup` 内部是一个 64 位原子计数器 + 一个信号量。`Done` 用原子操作减计数，减到 0 时释放信号量；`Wait` 阻塞在信号量上（`runtime_Semacquire`）——**全程无锁，原子操作级别**。

### 1.4 三个易错点

```go
// 坑 A：在 goroutine 里 Add —— Wait 可能先返回，漏等
var wg sync.WaitGroup
for i := 0; i < 5; i++ {
    go func() {
        wg.Add(1)   // ❌ 应该在外层 Add
        defer wg.Done()
    }()
}
wg.Wait()           // 可能计数器还没加上就返回了

// 坑 B：Done 少调用一次 → Wait 永远阻塞（和 goroutine 泄漏一样致命）
var wg sync.WaitGroup
wg.Add(1)
go func() {
    // 忘了 defer wg.Done()，或者提前 return 了
}()
wg.Wait()           // ❌ 永远等不到

// 坑 C：计数器减成负数 → panic
var wg sync.WaitGroup
wg.Add(1)
wg.Done()
wg.Done()           // ❌ panic: negative WaitGroup counter
```

### 1.5 其他同步原语（一句话清单）

| 原语 | 用途 |
|---|---|
| `sync.Mutex` / `sync.RWMutex` | 互斥锁 / 读写锁（保护共享数据） |
| `sync.Once` | 只执行一次（懒加载单例） |
| `sync.Map` | 并发安全的 map（读多写少场景） |
| `sync.Cond` | 条件变量（等"某个条件成立"再唤醒） |
| `golang.org/x/sync/errgroup` | WaitGroup 加强版：带错误收集和取消传播 |

> 注意：能用 channel 表达的场景优先用 channel（CSP 风格）；锁只用在真正需要保护共享内存的地方。

---

## 2. defer：延迟执行（Go 的"收尾神器"）

`defer` 把一句话**推迟到函数返回前执行**——资源清理、解锁、日志收尾全靠它。

### 2.1 基本语义：执行时机 + LIFO

```go
func demo() {
    defer fmt.Println("③ 最后执行")
    defer fmt.Println("② 倒数第二")
    fmt.Println("① 先执行")
}

// 输出：
// ① 先执行
// ② 倒数第二
// ③ 最后执行
```

两个关键规则：

1. **执行时机**：`return` 语句**先给返回值赋值**，然后才执行 defer，最后函数才真正返回
2. **LIFO（后进先出）**：多个 defer 按**栈式**逆序执行——先注册的后执行

### 2.2 最重要的坑：参数即时求值

**defer 后面的参数在 defer 语句执行的那一刻就求值了**，不是函数返回时：

```go
func demo() {
    start := time.Now()
    defer fmt.Println("耗时：", time.Since(start))  // ❌ 立刻求值 time.Since(start) ≈ 0！
    time.Sleep(1 * time.Second)
}
// 输出：耗时：0s（不是 1s！）
```

正确的计时姿势——**用闭包**（闭包捕获的是变量引用，返回时才取值）：

```go
func demo() {
    start := time.Now()
    defer func() { fmt.Println("耗时：", time.Since(start)) }()  // ✅ 返回时才算
    time.Sleep(1 * time.Second)
}
// 输出：耗时：1.000xxx s
```

同理，`defer file.Close()` 是立刻取 `file` 的值（没问题），但如果想在关闭前检查错误，要用闭包：

```go
defer func() {
    if err := file.Close(); err != nil {
        fmt.Println("关闭出错：", err)
    }
}()
```

### 2.3 命名返回值：defer 可以"偷改"返回值

```go
func demo() (result int) {
    defer func() { result += 100 }()   // 返回前给 result 加 100
    return 1                           // 先给 result 赋 1
}
// 实际返回值：101
```

原理：`return 1` 先把 1 赋给命名返回值 `result`，**然后**执行 defer（`result += 100` → 101），最后返回 101。命名返回值 + defer 是"函数结束时统一处理返回值"的惯用法（比如：给错误信息补上下文）。

### 2.4 defer + recover：Go 的"异常处理"

Go 没有 try-catch，用 `defer + recover` 捕获 panic：

```go
func safe() (err error) {
    defer func() {
        if r := recover(); r != nil {
            err = fmt.Errorf("捕获到 panic：%v", r)   // 借助命名返回值把 panic 转成 error
        }
    }()
    panic("炸了")   // 程序不会崩，err 会被设置
    return nil
}
```

注意：`recover` 只在 `defer` 的函数里有效，且**只能救自己这个 goroutine**——其他 goroutine 的 panic 救不了。

### 2.5 底层实现（一句话版）

`defer` 由编译器处理：注册时调用 `runtime.deferproc` 把"函数指针 + 参数"压入 goroutine 的 defer 链表；函数返回时 `runtime.deferreturn` 逆序弹出执行。**Go 1.14+ 做了 open-coded defer 优化**：大多数 defer 直接内联展开在返回路径上，不需要链表、不分配堆内存，性能从微秒级降到纳秒级——所以现在可以放心大胆地用 defer，不用为性能纠结。

### 2.6 三个易错点

```go
// 坑 A：循环里 defer —— 所有清理堆积到函数结束才执行（大循环会耗尽资源）
for i := 0; i < 100000; i++ {
    f, _ := os.Open(...)
    defer f.Close()   // ❌ 10 万个文件句柄攒到函数退出才关！
    // ✅ 正确：包一层函数，或用完立刻 f.Close()
}

// 坑 B：defer 的参数即时求值（见 2.2），闭包 vs 直接调用要分清

// 坑 C：defer 里 return —— 无效且迷惑
defer func() { return }()   // ❌ 只是退出闭包，不影响外层函数
```

### 2.7 最常用场景总结

| 场景 | 写法 |
|---|---|
| 解锁 | `defer mu.Unlock()` |
| 关文件 | `defer file.Close()` |
| WaitGroup | `defer wg.Done()` |
| 兜底 | `defer func(){ recover() }()` |
| 计时/日志 | `defer func(){ fmt.Println(time.Since(start)) }()` |

---

## 3. 实战：两者合体——带超时与错误收集的并发任务

把 WaitGroup + defer + recover 组合成一个真实场景：并发抓取多个 URL，任何一个 panic 都不能让程序崩，全部干完统一汇报：

```go
package main

import (
	"fmt"
	"sync"
)

func fetch(name string) (result string, err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("任务 %s panic：%v", name, r)   // ① defer+recover 兜底
		}
	}()
	if name == "坏任务" {
		panic("故意炸掉")                                     // 模拟故障
	}
	return "数据-" + name, nil
}

func main() {
	var wg sync.WaitGroup
	results := make([]string, 3)
	errs := make([]error, 3)

	for i, name := range []string{"任务1", "任务2", "坏任务"} {
		wg.Add(1)                                            // ② 启动前计数
		go func(idx int, n string) {
			defer wg.Done()                                  // ③ 收尾必减
			results[idx], errs[idx] = fetch(n)
		}(i, name)
	}

	wg.Wait()                                                // ④ 等全部干完
	for i, name := range []string{"任务1", "任务2", "坏任务"} {
		if errs[i] != nil {
			fmt.Printf("%s → 失败：%v\n", name, errs[i])
		} else {
			fmt.Printf("%s → %s\n", name, results[i])
		}
	}
}
```

运行输出（三个 goroutine 各干各的，坏任务被 defer+recover 救下，全部结束后统一汇报）：

```
任务1 → 数据-任务1
任务2 → 数据-任务2
坏任务 → 失败：任务 坏任务 panic：故意炸掉
```

（这段代码的真实输出已用 Go 1.24 实机验证过。）

---

## 4. 总结

| 工具 | 一句话 | 核心口诀 |
|---|---|---|
| `sync.WaitGroup` | 计数器等 goroutine | **Add 在外、Done 用 defer、Wait 收尾** |
| `defer` | 函数返回前必执行的收尾 | **LIFO 逆序、参数即时求值、命名返回值可被改** |

两者是黄金搭档：`defer wg.Done()` 一行代码，同时用上了"延迟执行"和"并发等待"两个机制——**保证无论 goroutine 怎么死，计数器都会减**。

回到主文章：[《Go 并发四件套：goroutine、channel、interface、select》](/posts/go-goroutine-channel-interface-select/)
