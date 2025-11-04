# ctapi-rs

[![Crates.io](https://img.shields.io/crates/v/ctapi-rs.svg)](https://crates.io/crates/ctapi-rs)
[![Documentation](https://docs.rs/ctapi-rs/badge.svg)](https://docs.rs/ctapi-rs)
[![Build Status](https://github.com/HTROY/ctapi-rs/workflows/CI/badge.svg)](https://github.com/HTROY/ctapi-rs/actions)

ctapi-rs 是一个安全、高性能的 Rust 库，用于与 Citect SCADA 系统的 CtAPI 进行交互。该库提供了完整的 API 包装，包括客户端连接管理、标签读写操作、对象搜索和属性获取等功能。

## 特性

- 🛡️ **内存安全**: 使用 Rust 的内存安全特性，避免缓冲区溢出等安全问题
- 🚀 **高性能**: 优化的字符串处理和编码转换，提高运行效率
- 📚 **完整文档**: 详细的 API 文档和使用示例
- 🧪 **测试覆盖**: 包含单元测试和集成测试
- 🔧 **易于使用**: 简洁的 API 设计，支持现代 Rust 最佳实践
- 🔄 **错误处理**: 强类型错误系统，提供详细的错误信息
- 🌏 **编码支持**: 完整的 GBK/UTF-8 编码转换支持

## 系统要求

- Rust 1.70 或更高版本
- Windows 操作系统
- Citect SCADA 系统（需要 CtAPI.dll）
- Visual C++ Redistributable

## 安装

在您的 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
ctapi-rs = "0.2.0"
```

## 快速开始

### 基本使用

```rust
use ctapi_rs::{CtClient, Result};

fn main() -> Result<()> {
    // 连接到本地 Citect SCADA
    let client = CtClient::open(None, None, None, 0)?;
    
    // 读取标签值
    let temperature = client.tag_read("Temperature")?;
    println!("温度: {}", temperature);
    
    // 写入标签值
    client.tag_write("Setpoint", 25.5)?;
    
    // 执行 Cicode 函数
    let time = client.cicode("Time(1)", 0, 0)?;
    println!("当前时间: {}", time);
    
    Ok(())
}
```

### 扩展标签读取

```rust
use ctapi_rs::{CtClient, CtTagValueItems, Result};

fn read_with_metadata() -> Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    let mut value_items = CtTagValueItems::default();
    
    let value = client.tag_read_ex("Pressure", &mut value_items)?;
    println!("压力值: {}", value);
    println!("时间戳: {}", value_items.timestamp);
    println!("质量: {}", value_items.quality_general);
    
    Ok(())
}
```

### 对象搜索

```rust
fn search_tags() -> Result<()> {
    let client = CtClient::open(None, None, None, 0)?;
    
    // 搜索特定集群的标签
    let results = client.find_first("Tag", "CLUSTER=Cluster1", None);
    
    for object in results {
        println!(
            "标签: {}, 注释: {}",
            object.get_property("TAG")?,
            object.get_property("COMMENT")?
        );
    }
    
    Ok(())
}
```

### 标签列表操作

```rust
fn use_tag_list() -> Result<()> {
    let mut client = CtClient::open(None, None, None, 0)?;
    let mut list = client.list_new(0)?;
    
    // 添加标签到列表
    list.add_tag("Tag1")?;
    list.add_tag("Tag2")?;
    
    // 批量读取
    list.read()?;
    
    // 获取标签值
    let value1 = list.read_tag("Tag1", 0)?;
    let value2 = list.read_tag("Tag2", 0)?;
    
    println!("标签值: {}, {}", value1, value2);
    
    Ok(())
}
```

## API 文档

详细的 API 文档可以在 [docs.rs](https://docs.rs/ctapi-rs) 上找到。

### 主要模块

- **[CtClient](https://docs.rs/ctapi-rs/latest/ctapi_rs/struct.CtClient.html)**: 主要客户端结构体
- **[CtList](https://docs.rs/ctapi-rs/latest/ctapi_rs/struct.CtList.html)**: 标签列表管理
- **[CtFind](https://docs.rs/ctapi-rs/latest/ctapi_rs/struct.CtFind.html)**: 对象搜索迭代器
- **[FindObject](https://docs.rs/ctapi-rs/latest/ctapi_rs/struct.FindObject.html)**: 搜索结果对象
- **[CtApiError](https://docs.rs/ctapi-rs/latest/ctapi_rs/enum.CtApiError.html)**: 错误类型定义

## 项目结构

```
ctapi-rs/
├── ctapi-sys/          # FFI 绑定层
├── ctapi-rs/           # 主要库代码
│   ├── src/
│   │   ├── client.rs   # 客户端实现
│   │   ├── find.rs     # 搜索功能
│   │   ├── list.rs     # 列表操作
│   │   ├── scaling.rs  # 工程单位转换
│   │   ├── error.rs    # 错误处理
│   │   └── constants.rs # 常量定义
├── examples/           # 使用示例
│   └── client/         # 客户端示例
└── README.md
```

## 开发

### 构建项目

```bash
git clone https://github.com/HTROY/ctapi-rs.git
cd ctapi-rs
cargo build
```

### 运行测试

```bash
cargo test
```

### 运行示例

```bash
cargo run --example client
```

## 错误处理

ctapi-rs 使用强类型的错误系统，提供详细的错误信息：

```rust
use ctapi_rs::CtApiError;

match client.tag_read("nonexistent_tag") {
    Ok(value) => println!("标签值: {}", value),
    Err(CtApiError::TagNotFound { tag }) => {
        println!("标签 '{}' 未找到", tag);
    }
    Err(CtApiError::System { code, message }) => {
        println!("系统错误 {}: {}", code, message);
    }
    Err(e) => {
        println!("其他错误: {}", e);
    }
}
```

## 性能优化

- **缓冲区管理**: 使用固定大小缓冲区避免动态分配
- **编码优化**: 高效的 GBK/UTF-8 编码转换
- **内存安全**: 避免不安全的内存操作
- **并发支持**: 线程安全的客户端实现

## 支持的 Citect 版本

- Citect SCADA 2018 及更高版本
- CtAPI v7.x 及更高版本

## 贡献

欢迎贡献！请查看我们的 [贡献指南](CONTRIBUTING.md) 了解详细信息。

### 开发环境设置

1. 安装 Rust 1.70+
2. 安装 Visual Studio Build Tools
3. 克隆仓库并运行 `cargo build`

### 代码规范

- 遵循 Rust 官方编码规范
- 添加适当的文档注释
- 确保所有测试通过
- 运行 `cargo fmt` 格式化代码

## 许可证

本项目采用 MIT 许可证。详情请查看 [LICENSE](LICENSE) 文件。

## 变更日志

详细的变更记录请查看 [CHANGELOG.md](CHANGELOG.md)。

## 支持

如果您遇到问题或有建议：

1. 查看 [GitHub Issues](https://github.com/HTROY/ctapi-rs/issues)
2. 创建新的 Issue
3. 参与讨论

## 致谢

感谢所有为这个项目做出贡献的开发者和用户。

---

**注意**: 本库需要 Citect SCADA 系统和相应的 CtAPI 运行时。请确保您的系统已正确安装和配置 Citect SCADA。
