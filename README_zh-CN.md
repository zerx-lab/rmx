<div align="center">

<pre>
 ██████╗ ███╗   ███╗██╗  ██╗
 ██╔══██╗████╗ ████║╚██╗██╔╝
 ██████╔╝██╔████╔██║ ╚███╔╝ 
 ██╔══██╗██║╚██╔╝██║ ██╔██╗ 
 ██║  ██║██║ ╚═╝ ██║██╔╝ ██╗
 ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝
</pre>

# rmx

**⚡ Windows 高性能并行目录删除工具**

*以闪电般的速度删除 `node_modules` 和 `target` 目录*

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows%2010%2B-0078D6?logo=windows)](https://www.microsoft.com/windows)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg?logo=rust)](https://www.rust-lang.org)

[English](./README.md) | [简体中文](./README_zh-CN.md)

</div>

---

## ✨ 性能对比

基准测试：5,301 个项目（5,000 个文件，301 个目录）

| 方法 | 耗时 | 对比 rmx |
|:----:|:----:|:--------:|
| **⚡ rmx** | **514ms** | **1.00x** |
| PowerShell Remove-Item | 1,150ms | 慢 2.2 倍 |

## 🚀 为什么 rmx 这么快？

| 特性 | 说明 |
|------|------|
| 🔥 **POSIX 删除语义** | 使用 `FILE_DISPOSITION_POSIX_SEMANTICS` 实现即时命名空间移除 |
| ⚡ **并行处理** | 多线程工作器配合依赖感知调度 |
| 🎯 **直接调用 API** | 绕过高层抽象，直接使用原生 Windows API |
| 📏 **长路径支持** | 使用 `\\?\` 前缀处理超过 260 字符的路径 |
| 🔄 **自动重试** | 对锁定文件采用指数退避重试策略 |

## 📦 安装

```bash
cargo install --path .
```

## 📖 使用方法

```bash
# 删除单个目录
rmx ./node_modules

# 删除多个目录
rmx ./target ./node_modules ./dist

# 预览模式（仅扫描不删除）
rmx -n ./node_modules

# 详细模式并显示统计信息
rmx -v --stats ./target

# 强制删除（跳过确认）
rmx --force ./path

# 强制终止占用文件的进程
rmx -rf --kill-processes ./locked_directory
```

## ⚙️ 命令选项

| 选项 | 说明 |
|------|------|
| `-r, -R, --recursive` | 递归删除目录及其内容 |
| `-f, --force` | 强制删除（跳过确认） |
| `-t, --threads <N>` | 工作线程数（默认：CPU 核心数） |
| `-n, --dry-run` | 仅扫描，不执行删除 |
| `-v, --verbose` | 显示进度和错误信息 |
| `--stats` | 显示详细统计信息 |
| `--no-preserve-root` | 不特殊处理根目录 |
| `--kill-processes` | 强制终止占用文件的进程（谨慎使用） |

## 🛡️ 安全特性

| 保护机制 | 说明 |
|----------|------|
| 🚫 系统目录保护 | 无法删除 `C:\Windows`、`C:\Program Files` 等系统目录 |
| 🏠 主目录保护 | 无法删除用户主目录 |
| 📂 当前目录检查 | 删除当前工作目录或其父目录时发出警告 |
| ✅ 确认机制 | 默认需要确认（使用 `-f` 跳过） |

## 🔧 技术细节

### Windows API 调用

- `CreateFileW` 配合 `FILE_SHARE_DELETE` 实现非阻塞访问
- `SetFileInformationByHandle` 配合 `FILE_DISPOSITION_INFORMATION_EX`
- `FILE_DISPOSITION_POSIX_SEMANTICS` 实现即时移除
- `FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE` 处理只读文件
- `FindFirstFileExW` / `FindNextFileW` 实现快速枚举

### 文件锁定处理

当文件被其他进程锁定时：
1. 最多重试 10 次，采用指数退避策略（10ms → 100ms）
2. 如果仍然锁定，记录失败并继续处理其他文件
3. 最后汇总报告所有失败项

## 📋 系统要求

- Windows 10 版本 1607 或更高版本
- NTFS 文件系统

## 📄 许可证

MIT OR Apache-2.0

---

<div align="center">

**[⬆ 返回顶部](#rmx)**

为 Windows 开发者用 ❤️ 打造

</div>
