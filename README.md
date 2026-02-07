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

**⚡ Fast Parallel Deletion for Windows**

*Quickly delete files, folders, and resolve file locking issues at blazing speed*

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows%2010%2B-0078D6?logo=windows)](https://www.microsoft.com/windows)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg?logo=rust)](https://www.rust-lang.org)

[English](./README.md) | [简体中文](./README_zh-CN.md)

</div>

---

## ✨ Performance

Benchmark on 5,301 items (5,000 files, 301 directories):

| Method | Time | vs rmx |
|:------:|:----:|:------:|
| **⚡ rmx** | **514ms** | **1.00x** |
| PowerShell Remove-Item | 1,150ms | 2.2x slower |

## 🚀 Key Features

| Feature | Description |
|---------|-------------|
| 📁 **Files & Folders** | Delete individual files, directories, and batch deletions seamlessly |
| 🔥 **POSIX Delete** | Uses `FILE_DISPOSITION_POSIX_SEMANTICS` for immediate namespace removal |
| ⚡ **Parallel** | Multi-threaded workers with dependency-aware scheduling |
| 🎯 **Direct API** | Bypasses high-level abstractions using native Windows API |
| 📏 **Long Paths** | Handles paths >260 characters with `\\?\` prefix |
| 🔄 **Auto Retry** | Exponential backoff for locked files |
| 🔓 **Delete Locked Items** | Terminate processes locking files/directories and delete them with `--kill-processes` |
| 🔓 **Unlock Items** | Unlock files/directories by closing handles and terminating locking processes without deletion using `--unlock` |

## 📦 Installation

### Scoop (Recommended)

```powershell
# Add the rmx bucket
scoop bucket add rmx https://github.com/zerx-lab/rmx

# Install rmx
scoop install rmx
```

### Cargo

```bash
cargo install --path .
```

### Manual Download

Download the latest release from [GitHub Releases](https://github.com/zerx-lab/rmx/releases).

## 📖 Usage

### Delete Directories

```bash
# Delete a single directory
rmx ./node_modules

# Delete multiple directories at once
rmx ./target ./node_modules ./dist

# Recursively delete directory and contents
rmx -r ./build_output
```

### Delete Files

```bash
# Delete a single file
rmx ./log.txt

# Delete multiple files
rmx ./file1.txt ./file2.log ./cache.db
```

### Delete Locked Files or Directories

```bash
# Terminate processes locking files/directories and delete them
rmx --kill-processes ./locked_directory

# Force termination, recursive deletion with locking process cleanup
rmx -rf --kill-processes ./path
```

### Unlock Files or Directories (Without Deleting)

```bash
# Only unlock a file without deleting it
rmx --unlock ./locked_file.txt

# Unlock a directory without deleting it
rmx --unlock ./locked_directory

# Unlock with verbose output to see process details
rmx --unlock -v ./path
```

### Preview & Safety

```bash
# Dry run (preview what would be deleted without actually deleting)
rmx -n ./node_modules

# Verbose mode with detailed statistics
rmx -v --stats ./target

# Force deletion (skip confirmation prompt)
rmx --force ./path
```

### Shell Extension

Initialize rmx shell extension for Windows Explorer right-click menu:

```powershell
# Initialize shell extension (install or reinstall)
rmx init
```

After initialization, right-click any file or folder to see "Delete with rmx" option.

**Note:** Run PowerShell or CMD as Administrator for the init command.

## ⚙️ Options

| Option | Description |
|--------|-------------|
| `-r, -R, --recursive` | Remove directories and their contents recursively |
| `-f, --force` | Force deletion without confirmation |
| `-t, --threads <N>` | Number of worker threads (default: CPU count) |
| `-n, --dry-run` | Scan but don't delete |
| `-v, --verbose` | Show progress and errors |
| `--stats` | Show detailed statistics |
| `--no-preserve-root` | Do not treat '/' specially |
| `--kill-processes` | Terminate processes locking files/directories, then delete them |
| `--unlock` | Only unlock files/directories (close handles) without deleting |

## 🛡️ Safety Features

| Protection | Description |
|------------|-------------|
| 🚫 System directories | Cannot delete `C:\Windows`, `C:\Program Files`, etc. |
| 🏠 Home directory | Cannot delete user's home directory |
| 📂 Current directory | Warns when deleting CWD or its parents |
| ✅ Confirmation | Asks for confirmation by default (use `-f` to skip) |

## 🔧 Technical Details

### Windows API Usage

- `CreateFileW` with `FILE_SHARE_DELETE` for non-blocking access
- `SetFileInformationByHandle` with `FILE_DISPOSITION_INFORMATION_EX`
- `FILE_DISPOSITION_POSIX_SEMANTICS` for immediate removal
- `FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE` for read-only files
- `FindFirstFileExW` / `FindNextFileW` for fast enumeration

### File Lock Handling

When a file is locked by another process:
1. Retry up to 10 times with exponential backoff (10ms → 100ms)
2. If still locked, record failure and continue with other files
3. Report all failures at the end

## 📋 Requirements

- Windows 10 version 1607 or later
- NTFS filesystem

## 📄 License

MIT

---

<div align="center">

**[⬆ Back to Top](#rmx)**

Made with ❤️ for Windows developers

</div>
