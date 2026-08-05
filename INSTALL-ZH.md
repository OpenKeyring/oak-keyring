# 安装 oak-keyring

oak-keyring 是 OpenKeyring 项目的 privacy-first、local-first 预览版密码管理器。命令行二进制文件名是 `ok`。

## 预览版支持边界

- 支持的操作系统：Apple Silicon 和 Intel 芯片的 macOS，以及 glibc 2.35 及以上版本的 Linux x86_64/ARM64（Ubuntu 22.04+、Debian 12+、Fedora、RHEL/Rocky/Alma 9+、Arch、openSUSE）。暂不支持 Alpine（musl）和 Windows。
- 在 Linux 上，内存锁定（`mlock`）可能需要调高 `RLIMIT_MEMLOCK`，详见下文「Linux 内存锁定」章节。
- 预览版构建未签名、未公证。macOS Gatekeeper 可能会在首次运行前提示风险。
- 本地 vault 和同步数据格式在稳定版前可能变化。预览版数据不提供兼容性保证。
- 社区支持通过 GitHub Issues 和 Discussions 尽力提供，不提供正式 SLA。

在不同预览版之间升级前，请先备份 vault 数据。

## GitHub Release 未签名构建

GitHub Release 是主要用户安装路径。Release 提供 macOS（Apple Silicon 和 Intel）与 Linux（x86_64 和 ARM64，glibc 2.35+）的未签名构建产物。

1. 打开最新 release：`https://github.com/OpenKeyring/oak-keyring/releases`。
2. 下载与你的操作系统和架构匹配的产物。
3. 解压后，把 `ok` 移动到 `PATH` 中的目录，例如 `/usr/local/bin` 或 `~/.local/bin`。
4. 验证安装：

```bash
ok --version
```

如果 macOS 阻止运行未签名预览版二进制，请先确认文件来自官方 GitHub Release 页面，再通过 Finder 或系统设置允许运行。

## Homebrew

```bash
brew tap openkeyring/oak-keyring
brew trust --formula openkeyring/oak-keyring/ok
brew install ok
```

Homebrew 6.0.0 及以上版本要求在加载非官方 tap 的 formula 之前先显式信任。该要求对 macOS 和 Linux 同时生效，并非只针对 Linux。如果跳过 `brew trust`，安装会报错：

```text
Error: Refusing to load formula openkeyring/oak-keyring/ok from untrusted tap openkeyring/oak-keyring.
Run `brew trust --formula openkeyring/oak-keyring/ok` or `brew trust openkeyring/oak-keyring` to trust it.
```

其他可选方式：

- 一步到位且只信任 `ok`（无需单独 `brew tap`/`brew trust`）：
  `brew install openkeyring/oak-keyring/ok`
- 信任整个 tap，而不是单个 formula：
  `brew trust openkeyring/oak-keyring`

## npm 内置二进制包

npm 包会为 macOS 和 Linux 安装内置的 `ok` 二进制文件：

```bash
npm install -g @openkeyring/ok
ok --version
```

如果 npm 包暂时不支持你的架构，请改用 GitHub Release。

## SSH Agent 后端（`ok agent`）

`ok agent` 启动一个独立的 ssh-agent 后端，使用 vault 中的 SSH 密钥。启动后
export 它打印的 `SSH_AUTH_SOCK=<path>`，然后 `ssh-add -l` 即可列出 vault 的
SSH 密钥（完整用法见 README）。

```bash
ok agent
# 输出：SSH_AUTH_SOCK=<path>
export SSH_AUTH_SOCK=<path>
ssh-add -l
```

`ok agent` 是一个长期运行的 daemon，会用 `mlock` 把机密数据锁定在内存中，因此
在 Linux 上同样需要满足下面的 `RLIMIT_MEMLOCK` 要求。

## Linux 内存锁定

`ok` 使用 `mlock` 把机密数据（主密钥、派生密钥等）锁定在内存中，使其不会被交换到磁盘。在 Linux 上，默认的 `RLIMIT_MEMLOCK` 通常只有 64 KiB，太小了。当 `mlock` 失败时，`ok` 会显式报错：创建或解锁 vault 会返回错误，而不是在没有内存保护的情况下静默运行。

在运行 `ok` 之前调高该限制。请根据你的环境选择合适的方式：

**交互式会话（临时）：**

```bash
ulimit -l unlimited
ok
```

**通过 PAM 持久化** — 在 `/etc/security/limits.conf` 中添加，然后重新登录：

```
*  soft  memlock  unlimited
*  hard  memlock  unlimited
```

**systemd 服务** — 在 unit 文件中添加：

```
[Service]
LimitMEMLOCK=infinity
```

**Capability（无需调整 ulimit）** — 一次性授予，重启后仍生效：

```bash
sudo setcap cap_ipc_lock=ep "$(command -v ok)"
```

macOS 不需要对普通用户施加同样的 `mlock` 配额，因此该步骤在 macOS 上不需要。

## 开发者源码构建

源码构建不是预览版的主要分发方式，因为当前构建会把用于同步的 Google OAuth2 配置编译进二进制。这个路径适合开发或源码审查。

前置条件：

- Apple Silicon 或 Intel 芯片的 macOS，或 Linux x86_64/ARM64（glibc 2.35+）
- 通过 `rustup` 安装的 Rust 工具链
- macOS 上：Xcode Command Line Tools。Linux 上：C 编译器和构建工具（`build-essential`/`gcc`/`make`）
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

> [!TIP]
> 建议：终端使用 Nerd Font，以确保图标正确显示。

## 更新

预览版升级前请先阅读 `CHANGELOG.md`。在稳定版本线发布前，数据格式兼容性不作保证。
