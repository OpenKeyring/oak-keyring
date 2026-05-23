# 安装 oak-keyring

oak-keyring 是 OpenKeyring 项目的 privacy-first、local-first 预览版密码管理器。命令行二进制文件名是 `ok`。

## 预览版支持边界

- 支持的操作系统：Apple Silicon 和 Intel 芯片的 macOS。
- 首个预览版暂不支持 Linux 和 Windows。
- 预览版构建未签名、未公证。macOS Gatekeeper 可能会在首次运行前提示风险。
- 本地 vault 和同步数据格式在稳定版前可能变化。预览版数据不提供兼容性保证。
- 社区支持通过 GitHub Issues 和 Discussions 尽力提供，不提供正式 SLA。

在不同预览版之间升级前，请先备份 vault 数据。

## GitHub Release 未签名构建

首个预览版的主要用户安装路径是 GitHub Release。Release 预计会提供 Apple Silicon 和 Intel macOS 的未签名构建产物。

1. 打开最新 release：`https://github.com/OpenKeyring/oak-keyring/releases`。
2. 下载与你的 Mac 架构匹配的产物。
3. 解压后，把 `ok` 移动到 `PATH` 中的目录，例如 `/usr/local/bin` 或 `~/.local/bin`。
4. 验证安装：

```bash
ok --version
```

如果 macOS 阻止运行未签名预览版二进制，请先确认文件来自官方 GitHub Release 页面，再通过 Finder 或系统设置允许运行。

## npm 内置二进制包

如果预览版发布了 npm 包，它应当为 macOS 安装内置的 `ok` 二进制文件：

```bash
npm install -g @openkeyring/ok
ok --version
```

如果 npm 包暂时不支持你的架构，请改用 GitHub Release。

## 开发者源码构建

源码构建不是预览版的主要分发方式，因为当前构建会把用于同步的 Google OAuth2 配置编译进二进制。这个路径适合开发或源码审查。

前置条件：

- Apple Silicon 或 Intel 芯片的 macOS
- 通过 `rustup` 安装的 Rust 工具链
- Xcode Command Line Tools
- 本地构建需要 Google OAuth 值，可以放在环境变量或 `.env` 中

```bash
git clone https://github.com/OpenKeyring/oak-keyring.git
cd oak-keyring
cp .env.example .env
# 编辑 .env，设置 OAK_GOOGLE_CLIENT_ID 和 OAK_GOOGLE_CLIENT_SECRET。
cargo build --release
./target/release/ok --version
```

如果只是做开发环境下的本地检查，且不会使用 Google Drive 同步，可以用占位 OAuth 值满足构建脚本。测试同步时需要真实 OAuth2 值。

## 更新

预览版升级前请先阅读 `CHANGELOG.md`。在稳定版本线发布前，数据格式兼容性不作保证。
