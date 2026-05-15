# oak-keyring 开发任务
# 使用: just <recipe>

# 列出所有可用 recipe
default:
    @just --list

# 编译 debug 版本
build:
    cargo build

# 编译 release 版本
release:
    cargo build --release

# 运行全部测试
test:
    cargo test

# 仅跑集成测试
test-integration:
    cargo test --test integration

# lint: 格式检查 + clippy
lint:
    cargo fmt --check
    cargo clippy -- -D warnings

# 一站式 PR 前检查 (lint + test)
check: lint test

# 格式化代码
fmt:
    cargo fmt
