# 线程安全指南

## 概述

ctapi-rs 提供了线程安全的 API，但需要遵循特定的使用模式以确保正确性。

## 线程安全性保证

### ✅ 线程安全的类型

#### `CtClient`

`CtClient` 实现了 `Send` 和 `Sync`，可以安全地在多线程间共享：

```rust
use std::sync::Arc;
use std::thread;
use ctapi_rs::CtClient;

let client = Arc::new(CtClient::open(None, None, None, 0)?);

// 可以在多个线程中安全使用
let handles: Vec<_> = (0..4).map(|i| {
    let client = Arc::clone(&client);
    thread::spawn(move || {
        client.tag_read(&format!("Tag_{}", i))
    })
}).collect();

for handle in handles {
    let result = handle.join().unwrap();
    println!("{:?}", result);
}
```

**保证**:
- ✅ 并发读取是安全的
- ✅ 并发写入是安全的（由底层 CtAPI.dll 同步）
- ✅ 读写混合是安全的
- ✅ Clone 操作是线程安全的

### ❌ 非线程安全的类型

#### `CtFind<'_>`

`CtFind` **不能**跨线程使用，它：
- 持有对 `CtClient` 的引用
- 在迭代过程中有内部可变状态
- 不实现 `Send` 或 `Sync`

**正确用法**:
```rust
let client = Arc::new(CtClient::open(None, None, None, 0)?);

let handles: Vec<_> = (0..4).map(|_| {
    let client = Arc::clone(&client);
    thread::spawn(move || {
        // ✅ 每个线程创建自己的 CtFind
        let results = client.find_first("Tag", "CLUSTER=Cluster1", None);
        for object in results {
            println!("{:?}", object.get_property("TAG"));
        }
        // CtFind 在线程结束前被 drop
    })
}).collect();

for handle in handles {
    handle.join().unwrap();
}
```

**错误用法**:
```rust
// ❌ 编译错误！CtFind 不实现 Send
let client = CtClient::open(None, None, None, 0)?;
let results = client.find_first("Tag", "", None);

thread::spawn(move || {
    for object in results {  // 错误：CtFind 不能跨线程
        // ...
    }
});
```

#### `CtList<'_>`

`CtList` 同样**不能**跨线程使用：
- 包含 `HashMap` 进行标签映射
- 方法需要 `&mut self`，暗示内部可变性
- 不实现 `Send` 或 `Sync`

**正确用法**:
```rust
let client = Arc::new(CtClient::open(None, None, None, 0)?);

let handles: Vec<_> = (0..4).map(|i| {
    let client = Arc::clone(&client);
    thread::spawn(move || {
        // ✅ 每个线程创建自己的 CtList
        let mut list = client.list_new(0).unwrap();
        list.add_tag(&format!("Tag_{}", i)).unwrap();
        list.read().unwrap();
        let value = list.read_tag(&format!("Tag_{}", i), 0).unwrap();
        println!("{}", value);
        // CtList 在线程结束前被 drop
    })
}).collect();

for handle in handles {
    handle.join().unwrap();
}
```

## 生命周期管理

### 关键规则

**规则 1**: 确保派生对象在客户端之前被释放

```rust
// ✅ 正确：CtFind 的生命周期短于 CtClient
let client = CtClient::open(None, None, None, 0)?;
{
    let results = client.find_first("Tag", "", None);
    for object in results {
        // 处理对象
    }
    // results (CtFind) 在此被 drop
}
// client 在此被 drop

// ❌ 危险：使用 Arc 时要小心
let client = Arc::new(CtClient::open(None, None, None, 0)?);
let results = client.find_first("Tag", "", None);
drop(client);  // 可能导致 use-after-free！
// results 仍然持有对已释放客户端的引用
```

**规则 2**: 在多线程环境中，确保所有线程完成后再释放客户端

```rust
use std::sync::Arc;

// ✅ 正确
let client = Arc::new(CtClient::open(None, None, None, 0)?);
let handles = vec![
    {
        let client = Arc::clone(&client);
        thread::spawn(move || {
            let results = client.find_first("Tag", "", None);
            for object in results {
                // 处理
            }
            // results 在线程结束前被 drop
        })
    }
];

for handle in handles {
    handle.join().unwrap();
}
// 所有 Arc<CtClient> 在此被 drop，引用计数降为 0
// CtClient 最后被 drop
```

## 常见陷阱

### 陷阱 1: 在 Arc 中提前释放

```rust
// ❌ 危险
let client = Arc::new(CtClient::open(None, None, None, 0)?);
let results = client.find_first("Tag", "", None);

thread::spawn({
    let client = Arc::clone(&client);
    move || {
        // 使用 client
    }
});

drop(results);  // CtFind dropped
drop(client);   // 主线程的 Arc dropped
// 如果线程仍在运行，可能有问题
```

### 陷阱 2: 在 unsafe 代码中延长生命周期

```rust
// ❌ 极度危险 - 不要这样做！
let client = CtClient::open(None, None, None, 0)?;
let results = client.find_first("Tag", "", None);

// 某些 unsafe 代码延长了 results 的生命周期
let leaked: &'static _ = unsafe { std::mem::transmute(&results) };

drop(client);  // CtClient dropped
drop(results); // CtFind dropped

// leaked 现在是悬垂引用！
```

## 性能考虑

### 连接池

对于高并发场景，考虑使用连接池：

```rust
use std::sync::{Arc, Mutex};

struct ConnectionPool {
    clients: Vec<Arc<CtClient>>,
    index: Mutex<usize>,
}

impl ConnectionPool {
    fn new(size: usize, computer: &str, user: &str, password: &str) -> Result<Self> {
        let clients = (0..size)
            .map(|_| CtClient::open(Some(computer), Some(user), Some(password), 0))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(Arc::new)
            .collect();
        
        Ok(Self {
            clients,
            index: Mutex::new(0),
        })
    }
    
    fn get_client(&self) -> Arc<CtClient> {
        let mut index = self.index.lock().unwrap();
        let client = Arc::clone(&self.clients[*index]);
        *index = (*index + 1) % self.clients.len();
        client
    }
}
```

### 读写分离

```rust
// 读密集型工作负载
let client = Arc::new(CtClient::open(None, None, None, 0)?);

// 多个读取线程
for i in 0..10 {
    let client = Arc::clone(&client);
    thread::spawn(move || {
        loop {
            let _ = client.tag_read(&format!("Tag_{}", i));
            thread::sleep(Duration::from_millis(100));
        }
    });
}

// 单个写入线程（如果需要）
let client_writer = Arc::clone(&client);
thread::spawn(move || {
    loop {
        let _ = client_writer.tag_write("ControlTag", 1);
        thread::sleep(Duration::from_secs(1));
    }
});
```

## 最佳实践总结

1. ✅ 使用 `Arc<CtClient>` 在线程间共享客户端
2. ✅ 每个线程创建自己的 `CtFind` 和 `CtList` 实例
3. ✅ 确保派生对象在客户端之前被 drop
4. ✅ 使用 `join()` 等待所有线程完成
5. ❌ 不要尝试跨线程传递 `CtFind` 或 `CtList`
6. ❌ 不要在 unsafe 代码中违反生命周期规则
7. ❌ 不要在有活动引用时 drop Arc<CtClient>

## FFI 层面的线程安全

根据 Citect SCADA 文档，底层 CtAPI.dll 对同一连接句柄的并发操作是线程安全的。这意味着：

- 多个线程可以同时在同一个 `CtClient` 上调用读操作
- 写操作由 DLL 内部同步
- 每个操作是原子的（从 FFI 边界的角度）

但是，这**不**意味着：
- 应用层面的操作序列是原子的
- 没有竞态条件（例如，读取-修改-写入序列）
- 不需要应用层的同步

如果需要复杂的同步模式，请在应用层使用 `Mutex` 或 `RwLock`。
