# oak-keyring

[English](README.md) | 简体中文

oak-keyring 是一个 privacy-first、local-first 的密码管理器，提供键盘驱动的终端 TUI 体验。

它面向习惯在终端中工作的用户：你可以在交互式全屏 TUI 中浏览、创建和编辑记录，生成密码，复制敏感字段，导入导出数据，并完成密码库恢复流程。

命令行二进制名是 `ok`。

## 预览版状态

oak-keyring 仍处于 pre-1.0 预览阶段。首个公开预览版用于试用产品、验证安装路径，并在后续稳定发布策略确定之前收集早期反馈。

重要边界：

- 不要把这个预览版当作唯一密码库。
- 首个预览版提供 Apple Silicon 和 Intel Mac 的 macOS 构建。
- 本预览版不包含 Linux 和 Windows 构建。
- macOS 预览版二进制未签名，也未 notarize。
- 密码库数据、配置和发布打包方式在后续稳定版之前可能变化。
- 没有正式支持 SLA。
- 你需要自己保管主密码、恢复词和备份。

## 为什么是终端 TUI？

很多密码工具提供适合脚本使用的 CLI，但日常密码库管理还需要浏览、选择、确认、恢复和状态反馈。oak-keyring 使用 TUI，让这些流程保持交互式、键盘驱动，并默认留在本地。

## 当前平台支持

首个预览版仅提供 macOS 构建：

| 平台 | Target |
| --- | --- |
| Apple Silicon Mac | `aarch64-apple-darwin` |
| Intel Mac | `x86_64-apple-darwin` |

产品方向不局限于这个首个预览版。Linux 和 Windows 构建不属于初始发布范围。

## 安装方式

推荐从 GitHub Release 直接下载未签名、未 notarize 的构建：

1. 下载与你的 Mac 架构匹配的 tarball。
2. 校验 `checksums.txt`。
3. 解压后运行 `ok --version`。

首个预览版的 GitHub Release 构建未签名，也未 notarize。macOS 首次运行时可能要求用户手动批准。ad-hoc 或自签名可以用于本地测试，但它不等同于 Apple Developer ID 签名和 notarization。

npm 便利安装：

```bash
npm install -g @openkeyring/ok
ok --version
```

npm 包预计会内置对应的 macOS 平台二进制。它是便利安装路径，不是主要安全信任根。

开发者源码构建：

```bash
git clone https://github.com/OpenKeyring/oak-keyring.git
cd oak-keyring
cp .env.example .env
# 编辑 .env，设置 OAK_GOOGLE_CLIENT_ID 和 OAK_GOOGLE_CLIENT_SECRET。
cargo build --release
./target/release/ok --version
```

源码构建不是预览版的主要分发方式，因为当前构建会把用于同步的 Google OAuth2 配置编译进二进制。源码构建适合开发或本地审查，并需要显式配置 OAuth2 值。

## 首次运行基础

启动应用：

```bash
ok
```

首次运行时创建 vault，设置强主密码，并把恢复词保存在安全位置。如果主密码和恢复词都丢失，维护者无法帮你恢复密码库。

## 安全与隐私预期

oak-keyring 是 local-first：vault 属于用户，默认保存在本机。正常 release build 使用 SQLCipher-backed 本地数据库。应用使用主密码和恢复词进行访问与恢复。

预览版不提供托管账户恢复服务。请把恢复词和备份保存在运行 oak-keyring 的设备之外。同步能力应按当前实际实现范围理解，不应理解为托管保管模式。

如果直接下载 release asset，请在运行二进制之前校验 checksum。安全问题报告见 [SECURITY.md](SECURITY.md)。

## 文档链接

- [SECURITY.md](SECURITY.md)：漏洞报告和安全边界。
- [LICENSE](LICENSE)：MIT license。
- 项目文档：Open-Keyring workspace 中的 `../docs/`。
- 网站源码：Open-Keyring workspace 中的 `../website/`。

## 项目状态

oak-keyring 是活跃开发中的预览项目。当前 release-readiness 工作聚焦于让首个 macOS 预览版清楚、可安装，并诚实说明限制；更广的平台支持和更强的发布保证会在后续阶段推进。
