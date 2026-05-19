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

## Directory Structure

```
tui/
├── screens/        # 各屏幕实现 (U1-U10)
├── components/     # 可复用 UI 组件 (12 个)
├── state/          # 状态管理 (per-screen state + shared state)
├── animation/      # 动画效果 (effects.rs, transitions.rs)
├── traits/         # Screen/Component trait 定义
├── i18n/           # 国际化 (rust-i18n)
├── mod.rs
├── terminal.rs     # 终端尺寸检测与响应式断点
└── theme.rs        # Tokyo Night 调色板
```

- **`mod.rs`**: module declarations + re-exports ONLY. Zero business logic.
- **Multi-file module**: `{name}/{mod, [domain files], tests}.rs`
- **Single-file with tests**: `file.rs` (business) + `file_test.rs` (tests) as siblings

## State Management

```
AppState
├── phase: AppPhase (Initializing/Locked/Unlocked/...)
├── shared: SharedState (notification/loading/focus/animation)
├── screens: ScreenStates (各屏幕状态实例)
├── current_screen: Screen
└── screen_history: Vec<ScreenSnapshot> (导航历史)
```

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
