# 异步 API

安全异步入口使用工作线程执行同步 CtAPI 调用。工作线程持有连接、列表和参数，直至原生函数返回。Tokio 用户可通过 `tokio-support` 使用 Tokio 的阻塞线程池。

## 手动收集结果

```rust,no_run
use ctapi_rs::{AsyncCtClient, AsyncOperation, CtClient};

fn main() -> ctapi_rs::error::Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    let mut operation = AsyncOperation::with_buffer_size(512);
    client.cicode_async("Time(1)", 0, 0, &mut operation)?;
    let result = operation.get_result(&client)?;
    println!("{result}");
    operation.reset()?;
    Ok(())
}
```

调用 `try_get_result` 可在工作未结束时得到 `None`。`is_complete` 只报告是否结束，操作是否成功由 `get_result` 或 `try_get_result` 返回。收集结果时使用发起调用的客户端或它的克隆。重置运行中的操作会返回 `WouldBlock`。

列表的 `read_async` 和 `write_tag_async` 使用相同的操作对象及收集接口。成功的列表操作返回空字符串。

## Future 和 Tokio

```rust,no_run
use ctapi_rs::{CtClient, FutureCtClient};
use std::sync::Arc;

async fn server_time(client: Arc<CtClient>) -> ctapi_rs::error::Result<String> {
    client.cicode_future("Time(1)", 0, 0)?.await
}
```

`FutureCtList` 提供带原生结果的列表 Future。使用 Tokio 时，可用 `TokioCtClient` 和 `TokioCtList` 将调用交给运行时的阻塞线程池。

## 超时和取消

`AsyncOperation::cancel` 只能取消尚未开始的请求。已经开始的调用会返回 `UnsupportedOperation`。Future 释放或超时会结束等待，已开始的调用仍可能完成并写入 SCADA。处理写入超时时，先通过应用的状态确认流程核对实际结果再决定是否重试。

低层原生异步接口由 `ctapi-sys` 提供。使用 `AsyncOperation::overlapped_mut` 的调用方必须自行管理原生操作、结果同步与内存生命周期。相关约束见 [异步实现](ASYNC_IMPLEMENTATION.md)。
