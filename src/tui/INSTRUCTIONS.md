# TUI Module (U1-U11)

基于 ratatui + crossterm 的终端界面层，遵循 TEA 架构。

## Core Traits

```rust
// Screen — 屏幕生命周期 (object-safe, 用于多态导航)
trait Screen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult;
    fn view(&self, frame: &mut Frame, area: Rect);
    fn on_mount(&mut self, ctx: &mut ScreenContext);
    fn on_unmount(&mut self);
}

enum ScreenResult { Continue, NavigateTo(Screen), PopScreen, Command(Box<Command>), ExitApp }

// Component — 可复用 UI 组件 (泛型关联状态)
trait Component {
    type State;
    fn update(state: &mut Self::State, msg: Message, ctx: &mut ScreenContext) -> Option<Command>;
    fn view(state: &Self::State, frame: &mut Frame, area: Rect);
}

// ScreenContext — 异步命令提交 + 配置访问
struct ScreenContext<'a> { command_tx: &'a Sender<Command>, config: &'a AppConfig }
```

## Message Flow

```
User Input → Message → Screen::update() → Command → Executor → CommandResult
     ↑                                                                    ↓
     └──────── Message::CommandCompleted ←───────────────────────────────┘
```

## Screens

| Spec | Screen | File |
|------|--------|------|
| U1 | UnlockScreen | `unlock.rs` |
| U1 | OnboardingScreen | `onboarding.rs` |
| U1 | RecoveryKeyScreen | `recovery_key.rs` |
| U1 | SetPasswordScreen | `set_password.rs` |
| U1 | ChangeMasterPasswordScreen | `change_master_password.rs` |
| U2 | MainScreen | `main/mod.rs`, `main/layout.rs` |
| U3 | ListPanel | `main/list.rs` |
| U4 | DetailPanel | `main/detail.rs` |
| U5 | OverlayManager | `main/overlay/` (help/confirm/password_history/batch_tag/error_dialog/generator) |
| U6 | PasswordGeneratorScreen | `password_generator.rs` |
| U7 | CreateRecord / EditRecord | `create_record.rs`, `edit_record.rs`, `form/` |
| U8 | ConfigScreen | `config_screen.rs`, `config_screen/config/` (about/general/security/sync/render) |
| U9 | ImportExportScreen | `import_export.rs` |
| U10 | AuditLogScreen | `audit_log.rs` |
| U10 | SyncConflictScreen | `sync_conflict.rs` |

## State Management

```
AppState
├── phase: AppPhase (Initializing/Locked/Unlocked/...)
├── shared: SharedState (notification/loading/focus/animation)
├── screens: ScreenStates (各屏幕状态实例)
├── current_screen: Screen
└── screen_history: Vec<ScreenSnapshot> (导航历史)
```

Per-screen state 文件:
`main_state.rs`, `list_state.rs`, `detail_state.rs`, `form_state.rs`,
`generator_state.rs`, `config_state.rs`, `audit_state.rs`, `sync_ui_state.rs`,
`tag_management.rs`, `overlay_state.rs`

## Components (13 个)

| Component | 用途 |
|-----------|------|
| TextInput | 带标签文本输入 (支持密码遮罩) |
| Dropdown | 下拉选择器 |
| TagInput | 标签输入 (带自动补全) |
| LengthSlider | 密码长度滑块 |
| StrengthBar | 密码强度指示器 |
| GeneratorPanel | 密码生成器面板 |
| SyncIndicator | 同步状态显示 |
| InlineValidation | 行内验证消息 |
| ProgressBar | 进度条 |
| Spinner | 加载旋转器 |
| EmptyState | 空状态提示 |
| VaultPathDialog | Vault 路径选择对话框 |

## Theme & Rendering

- **调色板**: Tokyo Night (theme.rs)
- **Style presets**: `Styles::focused_border()`, `Styles::error_text()`, `Styles::button_primary()` 等
- **Unicode 检查**: `unicode_capable` flag 控制图标和分隔符渲染
- **响应式断点** (terminal.rs): Full(≥120) / Medium(100-119) / Minimum(80-99) / TooSmall(<80)
- **最小终端尺寸**: 80x24

## Animation

```rust
enum AnimationLevel { Full, Reduced, None }  // 检测 COLORTERM/TERM_PROGRAM
```

- 效果通过 tachyonfx 构建 (effects.rs)
- 过渡常量 (transitions.rs): Unlock→Main 1000ms, PageSwitch 500ms, Modal 200ms
- 注意: tachyonfx 0.20.1 依赖 ratatui 0.29.x，slide/expand 效果暂用 dissolve 占位

## Keyboard Conventions

- **主屏幕**: Tab 循环面板焦点 (Sidebar → List → Detail)
- **Focus Stack**: overlay 弹出时 push_focus()，关闭时 pop_focus()
- **Visual Mode**: `v` 进入多选，Space 切换选择，`a` 全选
- **命令路由**: 根据焦点面板分发键盘事件
