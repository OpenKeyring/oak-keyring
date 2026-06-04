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

## 安装

### GitHub Release（推荐）

1. 下载与你的 Mac 架构匹配的 tarball。
2. 校验 `checksums.txt`。
3. 解压后运行 `ok --version`。

预览版构建未签名，也未 notarize。macOS 首次运行时可能需要手动批准。

### Homebrew

```bash
brew tap openkeyring/oak-keyring
brew install ok
```

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

## 预览版状态

oak-keyring 仍处于 pre-1.0 预览阶段（v0.8.0-preview.1）。

- 当前构建仅支持 macOS（Apple Silicon 和 Intel）；暂不提供 Linux 和 Windows 构建。
- macOS 二进制未签名，也未 notarize。
- 密码库数据、配置和发布打包方式在后续稳定版之前可能变化。
- 没有正式支持 SLA。
- 你需要自己保管主密码、恢复词和备份。

## 安全与隐私

oak-keyring 是 local-first：vault 属于用户，默认保存在本机。正常 release build 使用 SQLCipher-backed 本地数据库。应用使用主密码和恢复词进行访问与恢复。

预览版不提供托管账户恢复服务。请把恢复词和备份保存在运行 oak-keyring 的设备之外。同步能力应按当前实际实现范围理解，不应理解为托管保管模式。

如果直接下载 release asset，请在运行二进制之前校验 checksum。安全问题报告见 [SECURITY.md](SECURITY.md)。

## 链接

- [SECURITY.md](SECURITY.md) — 漏洞报告和安全边界
- [CONTRIBUTING.md](CONTRIBUTING.md) — 如何贡献
- [CHANGELOG.md](CHANGELOG.md) — 发布历史
- [LICENSE](LICENSE) — MIT license
