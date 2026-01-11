# ctapi-rs AI Coding Agent Instructions

## Project Overview

ctapi-rs 是一个安全、高性能的 Rust FFI 库，为 Citect SCADA 系统的 CtAPI (Windows-only) 提供内存安全的封装。该项目采用 workspace 结构，分为底层 FFI 绑定和高层安全抽象。

## Architecture

### 双层架构设计
- **ctapi-sys** (底层): 提供原始 FFI 绑定到 CtAPI.dll
  - 使用 `build.rs` 在编译时复制 x86/x64 DLL 文件到 target/deps
  - 定义 C 结构体：`CtTagValueItems`, `CtHScale`, `CtScale`
  - 不进行任何安全封装，仅提供 unsafe 接口
- **ctapi-rs** (高层): 提供安全的 Rust API
  - `client.rs`: 核心客户端实现，管理连接生命周期
  - `find.rs`: 对象搜索迭代器
  - `list.rs`: 标签列表批量操作
  - `scaling.rs`: 工程单位与原始值转换
  - `error.rs`: 强类型错误系统 (使用 thiserror)

### 关键设计决策
- **编码处理**: 所有字符串使用 `encoding_rs::GBK` 编码/解码，因为 Citect SCADA 使用 GBK 编码
  - `encode_to_gbk_cstring()`: Rust String → GBK CString
  - `extract_string_from_buffer()`: GBK 缓冲区 → UTF-8 String
- **内存安全**: 使用 `CStr::from_bytes_until_nul()` 安全处理 C 字符串
- **并发**: `CtClient` 实现 `Send + Sync`，支持多线程使用 (见 `multi_thread_test`)
  - ⚠️ **重要**: `CtFind` 和 `CtList` 不是 `Send/Sync`，每个线程需要创建自己的实例
  - 使用 `Arc<CtClient>` 在线程间共享客户端
  - 确保派生对象 (`CtFind`, `CtList`) 在 `CtClient` 被 drop 前释放
  - 详见 `THREAD_SAFETY.md` 文档

## Critical Workflows

### Building & Testing
```bash
# 构建整个 workspace
cargo build

# 运行测试 (需要 Citect SCADA 运行环境)
cargo test

# 运行特定示例
cargo run --example client
cargo run --example list-read
```

### FFI 层修改流程
修改 `ctapi-sys` 时：
1. 更新 `lib/{x64,x86}/ctapi.h` 头文件
2. 在 `ctapi-sys/src/lib.rs` 中添加对应的 Rust FFI 声明
3. `build.rs` 会自动处理 DLL 复制
4. 在 `ctapi-rs` 中添加安全封装

### 字符串处理模式
```rust
// 输入字符串转换
let c_string = encode_to_gbk_cstring(rust_str)?;

// 输出字符串解码
let mut buffer = [0i8; MAX_BUFFER_SIZE];
unsafe { ctapi_function(buffer.as_mut_ptr()) };
let result = extract_string_from_buffer(&buffer)?;
```

## Project-Specific Conventions

### 错误处理
- 使用 `thiserror::Error` 定义领域特定错误类型 (`CtApiError`)
- 公开 API 返回 `anyhow::Result<T>` 便于错误传播
- 所有 FFI 调用后立即检查返回值并转换为 Rust 错误

### 测试约定
- 集成测试使用硬编码测试服务器 (见 `lib.rs` 中的 `COMPUTER`, `USER`, `PASSWORD` 常量)
- 测试需要实际的 Citect SCADA 环境，无法进行模拟测试
- 使用 `#[test]` 而非 `tests/` 目录以便访问私有 API

### 命名模式
- API 函数名保持与 CtAPI C 接口一致 (如 `ctTagRead` → `tag_read`)
- 结构体使用 `Ct` 前缀对应 C 类型 (如 `CtTagValueItems`, `CtScale`)
- 常量使用 C 原始命名 (如 `CT_OPEN_RECONNECT`)

## Key Integration Points

### Windows 依赖
- 必须在 Windows 上编译和运行
- 需要 Visual C++ Redistributable
- 运行时需要 CtAPI.dll 在系统路径或与可执行文件同目录

### 外部依赖管理
- `encoding_rs`: GBK 编码转换
- `windows-sys`: Windows API 类型 (`OVERLAPPED`, `HANDLE`)
- `anyhow` + `thiserror`: 错误处理双重策略

## Examples & Documentation

- 所有公共 API 必须包含 rustdoc 注释和示例代码
- 示例代码位于 `examples/` workspace members
- README.md 包含快速开始指南和常见用法模式

## Common Pitfalls

1. **编码问题**: 忘记 GBK 编码转换会导致乱码
2. **缓冲区大小**: 使用 CtAPI 推荐的缓冲区大小常量 (见 `constants.rs`)
3. **生命周期**: `CtFind` 和 `CtList` 持有对 `CtClient` 的引用
4. **平台特定**: 所有 `#[cfg(windows)]` 检查，避免在非 Windows 平台编译失败

## Versioning & Release

- 遵循语义化版本 (当前 v0.2.0)
- 使用 `cliff.toml` 生成 `CHANGELOG.md`
- `ctapi-sys` 和 `ctapi-rs` 独立版本号
