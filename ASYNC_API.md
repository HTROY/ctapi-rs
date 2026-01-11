# 异步 API 使用指南

## 概述

ctapi-rs 提供了基于 Windows OVERLAPPED I/O 的异步操作支持，允许非阻塞地执行 Citect SCADA API 调用。

## 核心组件

### `AsyncOperation`

异步操作句柄，封装了 Windows `OVERLAPPED` 结构和结果缓冲区。

```rust
use ctapi_rs::AsyncOperation;

// 创建默认大小（256字节）的异步操作
let mut async_op = AsyncOperation::new();

// 创建自定义缓冲区大小的异步操作
let mut async_op = AsyncOperation::with_buffer_size(512);
```

### `AsyncCtClient` Trait

为 `CtClient` 添加异步方法的扩展 trait。

## 基本用法

### 1. 异步 Cicode 执行

```rust
use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};

let client = CtClient::open(None, None, None, 0)?;
let mut async_op = AsyncOperation::new();

// 启动异步操作
client.cicode_async("Time(1)", 0, 0, &mut async_op)?;

// 做其他工作...
println!("正在后台执行...");

// 阻塞等待结果
let result = async_op.get_result(&client)?;
println!("结果: {}", result);
```

### 2. 非阻塞轮询

```rust
let mut async_op = AsyncOperation::new();
client.cicode_async("LongRunningFunction()", 0, 0, &mut async_op)?;

loop {
    match async_op.try_get_result(&client) {
        Some(Ok(result)) => {
            println!("完成: {}", result);
            break;
        }
        Some(Err(e)) => {
            eprintln!("错误: {}", e);
            break;
        }
        None => {
            // 仍在运行，继续做其他工作
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
```

### 3. 检查完成状态

```rust
let mut async_op = AsyncOperation::new();
client.cicode_async("SomeFunction()", 0, 0, &mut async_op)?;

while !async_op.is_complete() {
    // 做其他工作
    println!("等待中...");
    std::thread::sleep(std::time::Duration::from_millis(50));
}

let result = async_op.get_result(&client)?;
```

### 4. 操作取消

```rust
let mut async_op = AsyncOperation::new();
client.cicode_async("Sleep(60)", 0, 0, &mut async_op)?;

// 决定取消
std::thread::sleep(std::time::Duration::from_millis(100));
async_op.cancel(&client)?;
```

## 异步列表操作

### 异步读取

```rust
let client = CtClient::open(None, None, None, 0)?;
let mut list = client.list_new(0)?;

list.add_tag("Temperature")?;
list.add_tag("Pressure")?;

// 启动异步读取
let mut async_op = AsyncOperation::new();
list.read_async(&mut async_op)?;

// 等待完成
while !async_op.is_complete() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}

// 读取标签值
let temp = list.read_tag("Temperature", 0)?;
let press = list.read_tag("Pressure", 0)?;
```

### 异步写入

```rust
let mut list = client.list_new(0)?;
list.add_tag("Setpoint")?;

let mut async_op = AsyncOperation::new();
list.write_tag_async("Setpoint", "25.5", &mut async_op)?;

// 等待写入完成
while !async_op.is_complete() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
```

## 高级模式

### 并发多个异步操作

```rust
let client = CtClient::open(None, None, None, 0)?;

// 创建多个异步操作
let mut ops = [
    AsyncOperation::new(),
    AsyncOperation::new(),
    AsyncOperation::new(),
];

let commands = ["Time(1)", "Date(4)", "Version()"];

// 启动所有操作
for (op, cmd) in ops.iter_mut().zip(commands.iter()) {
    client.cicode_async(cmd, 0, 0, op)?;
}

// 等待所有完成
for (i, op) in ops.iter_mut().enumerate() {
    match op.get_result(&client) {
        Ok(result) => println!("操作 {}: {}", i + 1, result),
        Err(e) => eprintln!("操作 {} 错误: {}", i + 1, e),
    }
}
```

### 重用 AsyncOperation

```rust
let mut async_op = AsyncOperation::new();

// 第一个操作
client.cicode_async("Time(1)", 0, 0, &mut async_op)?;
let result1 = async_op.get_result(&client)?;

// 重置并重用
async_op.reset();

// 第二个操作
client.cicode_async("Date(4)", 0, 0, &mut async_op)?;
let result2 = async_op.get_result(&client)?;
```

### 超时处理

```rust
use std::time::{Duration, Instant};

let mut async_op = AsyncOperation::new();
client.cicode_async("MayBeSlow()", 0, 0, &mut async_op)?;

let timeout = Duration::from_secs(5);
let start = Instant::now();

loop {
    if async_op.is_complete() {
        let result = async_op.get_result(&client)?;
        println!("完成: {}", result);
        break;
    }
    
    if start.elapsed() > timeout {
        async_op.cancel(&client)?;
        eprintln!("超时，已取消操作");
        break;
    }
    
    std::thread::sleep(Duration::from_millis(100));
}
```

## 性能优化

### 批量操作

对于需要执行多个独立操作的场景，使用异步 API 可以显著提升性能：

```rust
// 同步方式（串行执行）
let results: Vec<_> = (0..10)
    .map(|i| client.cicode(&format!("GetValue({})", i), 0, 0))
    .collect::<Result<Vec<_>, _>>()?;

// 异步方式（并发执行）
let mut ops: Vec<_> = (0..10).map(|_| AsyncOperation::new()).collect();
for (i, op) in ops.iter_mut().enumerate() {
    client.cicode_async(&format!("GetValue({})", i), 0, 0, op)?;
}
let results: Vec<_> = ops.iter_mut()
    .map(|op| op.get_result(&client))
    .collect::<Result<Vec<_>, _>>()?;
```

### 缓冲区管理

对于大结果，使用自定义缓冲区大小：

```rust
// 默认 256 字节可能不够
let mut async_op = AsyncOperation::with_buffer_size(4096);
client.cicode_async("GetLargeData()", 0, 0, &mut async_op)?;
```

## 线程安全注意事项

### ✅ 安全的做法

```rust
use std::sync::Arc;

let client = Arc::new(CtClient::open(None, None, None, 0)?);

let handles: Vec<_> = (0..4).map(|i| {
    let client = Arc::clone(&client);
    std::thread::spawn(move || {
        let mut async_op = AsyncOperation::new();
        client.cicode_async(&format!("Task({})", i), 0, 0, &mut async_op)?;
        async_op.get_result(&client)
    })
}).collect();

for handle in handles {
    let result = handle.join().unwrap()?;
    println!("结果: {}", result);
}
```

### ❌ 危险的做法

```rust
// 不要在线程间传递 AsyncOperation
let mut async_op = AsyncOperation::new();
client.cicode_async("Task()", 0, 0, &mut async_op)?;

// 错误！AsyncOperation 不是 Send
std::thread::spawn(move || {
    async_op.get_result(&client) // 编译错误
});
```

## 错误处理

### 常见错误码

- `ERROR_IO_PENDING (997)`: 操作正在进行中（正常）
- `ERROR_IO_INCOMPLETE (996)`: 操作尚未完成（正常）
- 其他错误码: 实际错误，需要处理

```rust
let mut async_op = AsyncOperation::new();

match client.cicode_async("Invalid()", 0, 0, &mut async_op) {
    Ok(_) => {
        // 操作已启动或立即完成
        match async_op.get_result(&client) {
            Ok(result) => println!("成功: {}", result),
            Err(e) => eprintln!("执行错误: {}", e),
        }
    }
    Err(e) => eprintln!("启动失败: {}", e),
}
```

## 最佳实践

1. ✅ **总是检查错误**: 不要假设操作会成功
2. ✅ **适当设置超时**: 防止无限等待
3. ✅ **重用 AsyncOperation**: 使用 `reset()` 避免重复分配
4. ✅ **并发执行**: 充分利用异步优势
5. ❌ **不要跨线程**: `AsyncOperation` 不是 `Send/Sync`
6. ❌ **不要忘记等待**: 确保操作完成后再访问结果
7. ❌ **不要过度使用**: 简单操作使用同步 API 即可

## 调试

使用 Debug trait 查看状态：

```rust
let async_op = AsyncOperation::new();
println!("{:?}", async_op);
// 输出: AsyncOperation { is_complete: false, buffer_size: 256 }
```

## 与 Rust async/await 的区别

⚠️ **重要**: ctapi-rs 的异步 API 基于 Windows OVERLAPPED I/O，**不是** Rust 的 async/await。

- ✅ 提供非阻塞操作
- ✅ 支持并发执行
- ❌ 不能与 `tokio` 或 `async-std` 直接配合
- ❌ 不返回 `Future`

如果需要与 Rust async 生态集成，可以使用 `tokio::task::spawn_blocking` 包装。

## 参考资料

- [CtAPI 文档](https://www.aveva.com/en/support/)
- [Windows OVERLAPPED I/O](https://docs.microsoft.com/en-us/windows/win32/sync/synchronization-and-overlapped-input-and-output)
- 完整示例: `examples/async-demo/`
