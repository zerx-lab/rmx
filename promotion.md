# rmx：给 Windows 换一个能用的删除

Windows 的文件删除功能有多难用，做开发的大概都有体会。

删个 node_modules 得等上好几秒甚至十几秒，进度条一格一格地挪。碰上某个文件被进程占用，直接弹个"操作无法完成，因为文件已在另一个程序中打开"，然后你得自己去找到底是哪个进程锁的，打开任务管理器翻一圈，杀掉再回来重试。

rmx 就是来解决这两件事的：**删得快**，**删得掉**。

## 它到底做了什么

rmx 是一个 Windows 下的命令行文件删除工具，Rust 写的，开源（MIT）。

速度上，它绕过了 Windows 的高层文件 API，直接调用底层的 `CreateFileW` + `SetFileInformationByHandle`，再配合 `FILE_DISPOSITION_POSIX_SEMANTICS` 这个标志位实现即时删除——文件在命名空间里直接消失，不用等所有句柄关闭。整个删除过程多线程并行，目录扫描和文件删除分层调度。

实际跑下来是什么效果？在 5301 个文件（5000 文件 + 301 目录）的测试里，rmx 用了 514 毫秒，PowerShell 的 `Remove-Item` 用了 1150 毫秒。快了一倍多。

文件占用这块更直接：加一个 `--kill-processes` 参数，rmx 通过 Windows Restart Manager API 自动识别锁住文件的进程，干掉它，再删。不用你自己去查。

## 不只是命令行

说实话，一个命令行删除工具对大多数人吸引力有限。rmx 真正有意思的地方在于它可以**直接替代 Windows 资源管理器的删除功能**。

跑一下 `rmx init`，它会注册一个 Shell 扩展到 Windows 右键菜单。之后你在资源管理器里右键任意文件或文件夹，会多出一个 "Delete with rmx" 的选项。

日常使用方式完全不变——还是右键、点删除，但背后走的是 rmx 的并行引擎。该快的快了，该能删的也能删了。对不想碰命令行的人来说，这才是真正有用的功能。

## 具体能干什么

**基本删除**

```bash
# 删文件夹
rmx ./node_modules

# 一次删多个
rmx ./target ./node_modules ./dist

# 删单个文件
rmx ./log.txt
```

**处理文件占用**

```bash
# 自动杀掉占用进程再删除
rmx --kill-processes ./locked_directory

# 递归 + 强制 + 杀进程，一把梭
rmx -rf --kill-processes ./path
```

```bash
# 只解除占用不删除（调试时有用）
rmx --unlock ./locked_file.txt
```

**右键菜单集成**

```powershell
# 注册到 Windows 资源管理器右键菜单（需要管理员权限）
rmx init
```

跑完之后就能右键删了，不用再开终端。

**其他**

```bash
# 预览模式，看看要删什么但不真删
rmx -n ./node_modules

# 查看删除统计
rmx -v --stats ./target

# 自升级
rmx upgrade
```

## 安全方面

rmx 内置了保护机制，删不了 `C:\Windows`、`C:\Program Files` 这些系统目录，也删不了用户主目录。没加 `-f` 的话删除前会要求确认。不用担心手滑把系统搞坏。

## 安装

最简单的方式是用 Scoop：

```powershell
scoop bucket add rmx https://github.com/zerx-lab/rmx
scoop install rmx
```

也可以用 Cargo：

```bash
cargo install --git https://github.com/zerx-lab/rmx
```

或者直接去 [GitHub Releases](https://github.com/zerx-lab/rmx/releases) 下载编译好的二进制。

装完建议跑一下 `rmx init` 把右键菜单注册上，日常用起来最方便。

## 技术要求

- Windows 10 1607 或更高版本
- NTFS 文件系统

## 谁适合用

- 前端开发，天天跟 node_modules 打交道的
- Rust 开发，target 文件夹动不动几个 G 的
- 任何经常碰到"文件被占用删不掉"的人
- 想要一个更快的右键删除的普通用户

---

GitHub：https://github.com/zerx-lab/rmx

协议：MIT
