# oak-keyring

[English](README.md) | 简体中文

[![Release](https://img.shields.io/github/v/release/OpenKeyring/oak-keyring?include_prereleases&label=release)](https://github.com/OpenKeyring/oak-keyring/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/OpenKeyring/oak-keyring/ci.yml?branch=develop&label=ci)](https://github.com/OpenKeyring/oak-keyring/actions/workflows/ci.yml?query=branch%3Adevelop)
[![npm](https://img.shields.io/npm/v/%40openkeyring%2Fok?label=npm)](https://www.npmjs.com/package/@openkeyring/ok)
[![License](https://img.shields.io/github/license/OpenKeyring/oak-keyring)](LICENSE)

oak-keyring 是一个 privacy-first、local-first 的密码管理器，提供键盘驱动的终端 TUI 体验。

很多密码工具提供适合脚本使用的 CLI，但日常密码库管理还需要浏览、选择、确认、恢复和状态反馈。oak-keyring 使用全屏 TUI，让这些流程保持交互式、键盘驱动，并默认留在本地。

命令行二进制名是 `ok`。

![TUI vault 浏览器：用键盘快捷键导航、编辑和复制凭证](examples/demo.gif)

## 功能

- **密码库管理** — 浏览、创建、编辑和删除凭证与安全笔记
- **密码生成器** — 独立使用或嵌入表单，可配置长度和字符集
- **键盘驱动 TUI** — 全屏界面，侧栏导航、搜索和批量操作
- **标签与回收站** — 用标签组织记录；软删除后可从回收站恢复
- **导入与导出** — 将数据导入或导出密码库
- **密码库恢复** — 使用 BIP-39 恢复词恢复访问
- **同步** — 可选的 Google Drive 云同步（预览）
- **自动锁定** — 不活动后自动锁定密码库
- **密码健康** — 泄露密码指示器和健康检查
- **macOS** — Apple Silicon 和 Intel 构建（预览）
- **Linux** — x86_64 和 ARM64 构建，glibc 2.35+（预览）

## 安装

### GitHub Release（推荐）

1. 下载与你的操作系统和架构匹配的 tarball。
2. 校验 `checksums.txt`。
3. 解压后运行 `ok --version`。

预览版构建未签名，也未 notarize。macOS 首次运行时可能需要手动批准。

### Homebrew

```bash
brew tap openkeyring/oak-keyring
brew trust --formula openkeyring/oak-keyring/ok
brew install ok
```

Homebrew 6.0+ 要求信任非官方 tap（macOS 与 Linux 均适用）。具体报错信息和可选方式见
[INSTALL-ZH.md](INSTALL-ZH.md)。

### npm

```bash
npm install -g @openkeyring/ok
ok --version
```

### 源码构建

```bash
git clone https://github.com/OpenKeyring/oak-keyring.git
cd oak-keyring
cp .env.example .env
# 编辑 .env，设置 OAK_GOOGLE_CLIENT_ID 和 OAK_GOOGLE_CLIENT_SECRET。
cargo build --release
./target/release/ok --version
```

源码构建会把用于同步的 Google OAuth2 配置编译进二进制。源码构建适合开发或本地审查，并需要显式配置 OAuth2 值。

> [!TIP]
> 建议：终端使用 Nerd Font，以确保图标正确显示。

## 快速开始

启动应用：

```bash
ok
```

首次运行时创建 vault，设置强主密码，并把恢复词保存在安全位置。如果主密码和恢复词都丢失，维护者无法帮你恢复密码库。

## 基本使用

oak-keyring 会打开一个全屏终端界面。主要使用流程是：

1. **创建或解锁 vault** — 运行 `ok`，首次使用时创建本地 vault，之后用主密码解锁已有 vault。
2. **浏览和搜索记录** — 通过侧边栏和记录列表浏览凭证、安全笔记、标签和回收站。使用 `Ctrl+K` 进入搜索模式，`Enter` 保留过滤结果，`Esc` 取消搜索。
3. **查看和复制秘密** — 选择记录后在详情面板查看字段。详情面板中，`c` 复制密码字段，`u` 复制用户名字段，`p` 在可用时显示或隐藏密码字段。
4. **创建和编辑记录** — 在非回收站状态下，`n` 创建新记录，`e` 编辑当前选中记录。
5. **生成密码** — 在主界面用 `Ctrl+G` 打开密码生成器；在记录表单中出现生成器时也可以直接使用。
6. **配置同步** — 用 `Ctrl+P` 打开 Config。Google Drive 同步是可选功能，并且仍属于预览版边界；配置完成后，可在主界面用 `Ctrl+R` 触发同步。
7. **导入和导出** — 使用 TUI 中的导入/导出流程迁移数据。导出的文件应按敏感数据处理。

当前网站文档见
[openkeyring.com/zh/docs/](https://openkeyring.com/zh/docs/)。

## SSH Agent 后端（`ok agent`）

`ok agent` 启动一个 ssh-agent 后端，使用 vault 中存储的 SSH 密钥。可用于
`ssh` 登录、通过 SSH 的 `git`，以及任何读取 `SSH_AUTH_SOCK` 的工具——**私钥
永远不会离开 oak-keyring 进程，主密码也永远不会进入 AI 工具或脚本。**

### 工作原理

1. daemon 解锁你的 vault（你输入主密码），然后监听一个 Unix socket。
2. `ssh` / `git` / AI 通过 `SSH_AUTH_SOCK` 发送签名请求；oak-keyring 在进程内
   签名，只返回签名结果。
3. 私钥每次签名时解密，签名后立即 zeroize——从不缓存，从不写入 vault 之外。

### 前置条件

- vault 中至少有一条 **SSH 密钥记录**。在 TUI 中创建：新记录（`n`）→ 类型选
  **SSH** → 粘贴公钥和 OpenSSH 私钥（若有 passphrase 一并填入）。
- 支持类型：**ed25519**、**RSA（SHA-2）**、**ECDSA（nistp256/384/521）**。

### 启动 agent

```bash
ok agent
```

它会提示输入主密码，然后打印 socket 路径，例如：

```
SSH_AUTH_SOCK=/run/user/1000/oak-keyring/agent.sock
```

### 配合 ssh 和 git 使用

在运行 `ssh` / `git` 的 shell 中 export 该路径：

```bash
export SSH_AUTH_SOCK=/run/user/1000/oak-keyring/agent.sock
ssh-add -l       # 列出 vault 中的 SSH 密钥
ssh user@host    # 用 vault 密钥认证——无需 ~/.ssh 密钥文件
git push         # git over SSH 同理
```

提示：每个会话启动一次 `ok agent`（或在 shell rc 中用固定 socket 路径启动）
并 export `SSH_AUTH_SOCK`，SSH 工具会自动找到它。

### 选项

| 参数 | 用途 |
| --- | --- |
| `--only NAME` | 只暴露名称精确匹配的记录（可重复）。 |
| `--allow REGEX` | 额外暴露名称匹配该正则的记录（与 `--only` 取并集）。 |
| `--idle-lock SECS` | 无成功签名超过该秒数后自动关闭（默认：永不）。 |

运行 `ok agent --help` 查看完整列表。

### 安全模型

- **私钥永不离开 daemon**——只有签名结果经过 socket。
- **不缓存密钥**——解密后的私钥每次签名后立即 zeroize。
- **主密码隔离**——通过终端提示读取一次；不进入命令行参数、shell 历史或任何
  AI 工具的上下文。
- **每次签名审计**——每次成功签名记入 vault 审计日志（`SSH sign`）。
- **与 TUI 并存**——独立的单实例锁，`ok` 和 `ok agent` 可同时对同一个 vault
  运行。

> 本地信任模型：以你的用户身份运行的进程本就能读你的文件，因此 socket 权限为
> `0600`。相比普通 `ssh-agent`，好处是 SSH 私钥始终在 vault 中加密静态存储，
> 永不作为明文文件写入 `~/.ssh`。

### 停止 agent

`ok agent` 在前台运行。用 `Ctrl+C` 或 `kill <pid>`（`SIGTERM` / `SIGINT`）
停止。关闭时会锁定 vault（zeroize 密钥）并删除 socket 和 pidfile。

### 故障排查

- **`ssh-add -l`："Could not open a connection"**——当前 shell 未 export
  `SSH_AUTH_SOCK`，或指向过期路径。重新 export agent 打印的路径。
- **`ssh-add -l` 列不出东西**——vault 中没有 SSH 密钥记录，或全被 `--only` /
  `--allow` 过滤掉。
- **"another agent is already running"**——已有一个 `ok agent` 在跑；先停止它
  （或删除数据目录里过期的 `.agent.lock`）。
- **崩溃后残留 socket**——`ok agent` 下次启动会清理残留 socket；也可手动删除。
- **Linux 内存锁定错误**——调高 `RLIMIT_MEMLOCK`（见 [INSTALL-ZH.md](INSTALL-ZH.md)）。

## 预览版状态

oak-keyring 仍处于 pre-1.0 预览阶段（v0.8.0-preview.3）。

- 当前构建支持 macOS（Apple Silicon 和 Intel）和 Linux（x86_64/ARM64，glibc 2.35+）；暂不提供 Windows 构建。Linux 上可能需要调高 `mlock` 的 `RLIMIT_MEMLOCK`（见 INSTALL-ZH.md）。
- macOS 二进制未签名，也未 notarize。
- 密码库数据、配置和发布打包方式在后续稳定版之前可能变化。
- 没有正式支持 SLA。
- 你需要自己保管主密码、恢复词和备份。

## 安全与隐私

oak-keyring 是 local-first：vault 属于用户，默认保存在本机。正常 release build 使用 SQLCipher-backed 本地数据库。应用使用主密码和恢复词进行访问与恢复。

预览版不提供托管账户恢复服务。请把恢复词和备份保存在运行 oak-keyring 的设备之外。同步能力应按当前实际实现范围理解，不应理解为托管保管模式。

如果直接下载 release asset，请在运行二进制之前校验 checksum。安全问题报告见 [SECURITY.md](SECURITY.md)。

## 链接

- [网站文档](https://openkeyring.com/zh/docs/) — 安装、使用、快捷键、安全和预览版状态
- [SECURITY.md](SECURITY.md) — 漏洞报告和安全边界
- [THREAT_MODEL.md](THREAT_MODEL.md) — 安全假设、非目标以及威胁边界
- [PRIVACY.md](PRIVACY.md) — 本地数据处理、可选同步、遥测以及隐私界限
- [CONTRIBUTING.md](CONTRIBUTING.md) — 如何贡献
- [CHANGELOG.md](CHANGELOG.md) — 发布历史
- [LICENSE](LICENSE) — MIT license
