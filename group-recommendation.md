# 群推荐话术

---

## 版本一（简短直接）

分享一个 Windows 删除工具 rmx，Rust 写的，开源。

用了一段时间，说下实际感受：Windows 自带的删除太拉了。删个 node_modules 要等半天不说，碰到文件被占用直接给你报错卡住。rmx 基本上就是为了解决这两个问题——删得快，删得掉。

实测删 5000 多个文件只要 500 毫秒左右，PowerShell 的 Remove-Item 要 1.1 秒。文件被进程占用的话，加个 `--kill-processes` 参数，它会自动把占用进程干掉再删。

另外有个我觉得比较实用的点：跑一下 `rmx init` 就能注册到资源管理器右键菜单，直接替代 Windows 原生的删除。日常用起来跟以前一样右键删，但快了一倍多，被占用的文件也不会再卡你了。

GitHub：https://github.com/zerx-lab/rmx

---

## 版本二（偏口语/群聊风格）

有没有人跟我一样受不了 Windows 删文件夹慢的？

前端项目一多，删 node_modules 真的要命，进度条一卡一卡的，碰上文件占用还删不掉。

最近在用一个叫 rmx 的命令行工具，Rust 写的，开源。实际跑下来删 5000 多个文件大概半秒就搞定了，比 PowerShell 快一倍多。最关键的是文件被占用它也能删——自动找到占用的进程杀掉。

而且它可以注册到右键菜单，跑个 `rmx init` 就行。之后右键删除走的就是 rmx，不用开终端，日常用跟 Windows 原来的删除一样方便，但该卡的地方全不卡了。

地址：https://github.com/zerx-lab/rmx
装的话推荐用 scoop：`scoop bucket add rmx https://github.com/zerx-lab/rmx && scoop install rmx`

---

## 版本三（技术群/开发者群）

推荐一个 Windows 下的文件删除工具 rmx，Rust 实现，MIT 协议。

底层用的 `FILE_DISPOSITION_POSIX_SEMANTICS`，文件从命名空间里立刻移除，不用等句柄释放。删除过程全程多线程并行，目录树扫描和文件删除分开走。实测 5301 个文件 514ms，PowerShell Remove-Item 同样内容 1150ms。

有两个比较实用的功能：

1. `--kill-processes` 自动通过 Restart Manager API 定位占用进程并终止，再执行删除。做开发的应该都碰到过"文件被另一个程序使用"删不掉的情况，这个参数直接解决。
2. `--unlock` 只释放句柄不删除文件，调试的时候有用。

支持注册到 Windows 资源管理器右键菜单（`rmx init`），可以直接替代系统默认删除。长路径、只读文件、pnpm 硬链接这些边界情况都处理了。

GitHub：https://github.com/zerx-lab/rmx
