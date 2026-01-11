# Tokio 运行时集成指南

## 概述

ctapi-rs 提供了与 Tokio 异步运行时的完整集成，允许使用 Rust 标准的 `async/await` 语法进行 Citect SCADA API 调用。

## 启用 Tokio 支持

在 `Cargo.toml` 中启用 `tokio-support` feature：

```toml
[dependencies]
ctapi-rs = { version = "0.2", features = ["tokio-support"] }
tokio = { version = "1", features = ["full"] }
```

## 核心 API

### `TokioCtClient` Trait

为 `CtClient` 和 `Arc<CtClient>` 提供 async/await 方法：

```rust
pub trait TokioCtClient {
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String>;
    async fn tag_read_tokio(&self, tag: &str) -> Result<String>;
    async fn tag_write_tokio(&self, tag: &str, value: impl Into<String> + Send) -> Result<()>;
}
```

### `TokioCtList` Trait

为 `CtList` 提供异步批量操作：

```rust
pub trait TokioCtList {
    async fn read_tokio(&mut self) -> Result<()>;
}
```

## 基本用法

### 1. 简单的 async/await 调用

```rust
use ctapi_rs::{CtClient, TokioCtClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    
    // 直接使用 .await
    let time = client.cicode_tokio("Time(1)", 0, 0).await?;
    println!("当前时间: {}", time);
    
    Ok(())
}
```

### 2. 异步标签读写

```rust
use ctapi_rs::{CtClient, TokioCtClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    
    // 异步读取
    let temp = client.tag_read_tokio("Temperature").await?;
    println!("温度: {}", temp);
    
    // 异步写入
    client.tag_write_tokio("Setpoint", "25.5").await?;
    
    Ok(())
}
```

### 3. 并发操作

```rust
use ctapi_rs::{CtClient, TokioCtClient};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(CtClient::open(None, None, None, 0)?);
    
    // 同时启动多个操作
    let time_future = client.cicode_tokio("Time(1)", 0, 0);
    let date_future = client.cicode_tokio("Date(4)", 0, 0);
    let version_future = client.cicode_tokio("Version()", 0, 0);
    
    // 并发等待所有结果
    let (time, date, version) = tokio::try_join!(
        time_future,
        date_future,
        version_future
    )?;
    
    println!("时间: {}, 日期: {}, 版本: {}", time, date, version);
    Ok(())
}
```

### 4. 使用 tokio::spawn 启动任务

```rust
use ctapi_rs::{CtClient, TokioCtClient};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(CtClient::open(None, None, None, 0)?);
    
    let mut handles = vec![];
    
    // 启动 10 个并发任务
    for i in 0..10 {
        let client = Arc::clone(&client);
        let handle = tokio::spawn(async move {
            client.cicode_tokio(&format!("GetValue({})", i), 0, 0).await
        });
        handles.push(handle);
    }
    
    // 等待所有任务完成
    for handle in handles {
        let result = handle.await??;
        println!("结果: {}", result);
    }
    
    Ok(())
}
```

### 5. 异步列表操作

```rust
use ctapi_rs::{CtClient, TokioCtList};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    let mut list = client.list_new(0)?;
    
    list.add_tag("Temperature")?;
    list.add_tag("Pressure")?;
    list.add_tag("FlowRate")?;
    
    // 异步读取所有标签
    list.read_tokio().await?;
    
    // 获取值
    let temp = list.read_tag("Temperature", 0)?;
    let press = list.read_tag("Pressure", 0)?;
    let flow = list.read_tag("FlowRate", 0)?;
    
    println!("温度: {}, 压力: {}, 流量: {}", temp, press, flow);
    Ok(())
}
```

## 高级模式

### 超时处理

```rust
use tokio::time::{timeout, Duration};
use ctapi_rs::{CtClient, TokioCtClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    
    // 设置 5 秒超时
    match timeout(
        Duration::from_secs(5),
        client.cicode_tokio("MayBeSlow()", 0, 0)
    ).await {
        Ok(Ok(result)) => println!("结果: {}", result),
        Ok(Err(e)) => eprintln!("操作失败: {}", e),
        Err(_) => eprintln!("操作超时"),
    }
    
    Ok(())
}
```

### Select 操作

```rust
use tokio::time::{sleep, Duration};
use ctapi_rs::{CtClient, TokioCtClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    
    tokio::select! {
        result = client.cicode_tokio("Task1()", 0, 0) => {
            println!("Task1 完成: {:?}", result);
        }
        _ = sleep(Duration::from_secs(5)) => {
            println!("超时");
        }
    }
    
    Ok(())
}
```

### 流式处理

```rust
use ctapi_rs::{CtClient, TokioCtClient};
use futures::stream::{self, StreamExt};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(CtClient::open(None, None, None, 0)?);
    
    let tags = vec!["Tag1", "Tag2", "Tag3", "Tag4", "Tag5"];
    
    // 使用流并发处理，限制并发数为 3
    let results: Vec<_> = stream::iter(tags)
        .map(|tag| {
            let client = Arc::clone(&client);
            async move {
                let value = client.tag_read_tokio(tag).await?;
                Ok::<_, anyhow::Error>((tag, value))
            }
        })
        .buffer_unordered(3)  // 最多 3 个并发
        .collect()
        .await;
    
    for result in results {
        match result {
            Ok((tag, value)) => println!("{}: {}", tag, value),
            Err(e) => eprintln!("错误: {}", e),
        }
    }
    
    Ok(())
}
```

### 定时轮询

```rust
use tokio::time::{interval, Duration};
use ctapi_rs::{CtClient, TokioCtClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    
    let mut interval = interval(Duration::from_secs(1));
    
    loop {
        interval.tick().await;
        
        match client.tag_read_tokio("Temperature").await {
            Ok(value) => println!("温度: {}", value),
            Err(e) => eprintln!("读取失败: {}", e),
        }
    }
}
```

## 实现原理

### spawn_blocking 包装

Tokio 集成使用 `tokio::task::spawn_blocking` 将阻塞的 CtAPI 调用包装为异步操作：

```rust
async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
    let client = Arc::new(self.clone());
    let cmd = cmd.to_string();
    
    tokio::task::spawn_blocking(move || {
        let mut async_op = AsyncOperation::new();
        client.cicode_async(&cmd, vh_win, mode, &mut async_op)?;
        async_op.get_result(&client)
    })
    .await
    .map_err(|e| CtApiError::Other(e.to_string()))?
}
```

### 线程池管理

- 阻塞操作运行在 Tokio 的 blocking 线程池中
- 不会阻塞异步运行时的工作线程
- 自动管理线程池大小和资源

## 性能优化

### 1. 合理的并发度

```rust
// ✅ 好：限制并发数
use futures::stream::{self, StreamExt};

stream::iter(tags)
    .map(|tag| async move { /* ... */ })
    .buffer_unordered(10)  // 限制为 10 个并发
    .collect()
    .await;

// ❌ 避免：无限并发
let futures: Vec<_> = tags.iter()
    .map(|tag| client.tag_read_tokio(tag))
    .collect();
futures::future::join_all(futures).await;
```

### 2. 连接池

对于多客户端场景，使用连接池：

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

struct ClientPool {
    clients: Vec<Arc<CtClient>>,
    semaphore: Arc<Semaphore>,
}

impl ClientPool {
    fn new(size: usize) -> Result<Self> {
        let clients = (0..size)
            .map(|_| CtClient::open(None, None, None, 0).map(Arc::new))
            .collect::<Result<Vec<_>>>()?;
        
        Ok(Self {
            clients,
            semaphore: Arc::new(Semaphore::new(size)),
        })
    }
    
    async fn with_client<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Arc<CtClient>) -> T + Send + 'static,
        T: Send + 'static,
    {
        let permit = self.semaphore.acquire().await.unwrap();
        let client = self.clients[0].clone();  // 简化示例
        
        let result = tokio::task::spawn_blocking(move || {
            let res = f(client);
            drop(permit);
            res
        }).await?;
        
        Ok(result)
    }
}
```

### 3. 批量操作

使用 `CtList` 批量操作：

```rust
// ✅ 好：批量读取
let mut list = client.list_new(0)?;
for tag in &tags {
    list.add_tag(tag)?;
}
list.read_tokio().await?;

// ❌ 避免：逐个读取
for tag in &tags {
    client.tag_read_tokio(tag).await?;
}
```

## 对比：OVERLAPPED vs Tokio

| 特性 | OVERLAPPED API | Tokio API |
|------|---------------|-----------|
| 语法 | 手动轮询/等待 | async/await |
| 并发模型 | 需要手动管理 | 自动任务调度 |
| 取消支持 | 显式取消 | Drop 时取消 |
| 超时处理 | 手动实现 | `tokio::time::timeout` |
| 生态集成 | 无 | 完整 tokio 生态 |
| 性能开销 | 最低 | 轻微（spawn_blocking） |
| 适用场景 | 性能关键代码 | 业务逻辑代码 |

## 线程安全

### ✅ 安全的做法

```rust
// 使用 Arc 共享客户端
let client = Arc::new(CtClient::open(None, None, None, 0)?);

// 在多个任务间共享
let client1 = Arc::clone(&client);
tokio::spawn(async move {
    client1.tag_read_tokio("Tag1").await
});

let client2 = Arc::clone(&client);
tokio::spawn(async move {
    client2.tag_read_tokio("Tag2").await
});
```

### ❌ 避免的做法

```rust
// 不要在任务间移动 AsyncOperation
let mut async_op = AsyncOperation::new();
tokio::spawn(async move {
    async_op.get_result(&client)  // 编译错误：AsyncOperation 不是 Send
});
```

## 错误处理

### 模式匹配

```rust
match client.tag_read_tokio("Temperature").await {
    Ok(value) => println!("温度: {}", value),
    Err(CtApiError::TagNotFound(tag)) => eprintln!("标签不存在: {}", tag),
    Err(CtApiError::System(code, msg)) => eprintln!("系统错误 {}: {}", code, msg),
    Err(e) => eprintln!("其他错误: {}", e),
}
```

### anyhow 集成

```rust
use anyhow::Context;

async fn read_critical_tag(client: &CtClient) -> anyhow::Result<String> {
    client.tag_read_tokio("CriticalTag")
        .await
        .context("Failed to read critical tag")
}
```

## 最佳实践

1. ✅ **使用 Arc 共享客户端**：避免不必要的克隆
2. ✅ **合理限制并发**：使用 `buffer_unordered` 或 Semaphore
3. ✅ **设置超时**：防止无限等待
4. ✅ **优雅关闭**：确保所有任务完成后再退出
5. ❌ **避免阻塞 async 上下文**：不要在 async 函数中使用同步 API
6. ❌ **不要过度并发**：考虑 Citect SCADA 服务器负载

## 调试

启用 tokio 控制台：

```toml
[dependencies]
tokio = { version = "1", features = ["full", "tracing"] }
console-subscriber = "0.1"
```

```rust
#[tokio::main]
async fn main() {
    console_subscriber::init();
    // ...
}
```

## 完整示例

参见 `examples/tokio-demo/` 获取完整的工作示例，包括：
- 基本 async/await 用法
- 并发操作
- 超时处理
- 任务生成
- 错误处理
- 列表操作

## 参考资料

- [Tokio 官方文档](https://tokio.rs)
- [Async Rust 书籍](https://rust-lang.github.io/async-book/)
- [ASYNC_API.md](ASYNC_API.md) - OVERLAPPED API 文档
