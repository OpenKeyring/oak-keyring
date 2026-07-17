# TUI Module (U1-U11)

Terminal interface layer built on ratatui + crossterm, following the TEA architecture.

## Core Traits

```rust
// Screen — screen lifecycle (object-safe, for polymorphic navigation)
trait Screen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult;
    fn view(&self, frame: &mut Frame, area: Rect);
    fn on_mount(&mut self, ctx: &mut ScreenContext);
    fn on_unmount(&mut self);
}

enum ScreenResult { Continue, NavigateTo(Screen), PopScreen, Command(Box<Command>), ExitApp }

// Component — reusable UI component (generic associated state)
trait Component {
    type State;
    fn update(state: &mut Self::State, msg: Message, ctx: &mut ScreenContext) -> Option<Command>;
    fn view(state: &Self::State, frame: &mut Frame, area: Rect);
}

// ScreenContext — async command submission + config access
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
├── screens/        # per-screen implementations (U1-U10)
├── components/     # reusable UI components (12 total)
├── state/          # state management (per-screen state + shared state)
├── animation/      # animation effects (effects.rs, transitions.rs)
├── traits/         # Screen/Component trait definitions
├── i18n/           # internationalization (rust-i18n)
├── mod.rs
├── terminal.rs     # terminal size detection and responsive breakpoints
└── theme.rs        # Tokyo Night palette
```

- **`mod.rs`**: module declarations + re-exports ONLY. Zero business logic.
- **Multi-file module**: `{name}/{mod, [domain files], tests}.rs`
- **Single-file with tests**: `file.rs` (business) + `file_test.rs` (tests) as siblings

## State Management

```
AppState
├── phase: AppPhase (Initializing/Locked/Unlocked/...)
├── shared: SharedState (notification/loading/focus/animation)
├── screens: ScreenStates (per-screen state instances)
├── current_screen: Screen
└── screen_history: Vec<ScreenSnapshot> (navigation history)
```

## Theme & Rendering

- **Palette**: Tokyo Night (theme.rs)
- **Style presets**: `Styles::focused_border()`, `Styles::error_text()`, `Styles::button_primary()`, etc.
- **Unicode check**: `unicode_capable` flag controls icon and separator rendering
- **Responsive breakpoints** (terminal.rs): Full(≥120) / Medium(100-119) / Minimum(80-99) / TooSmall(<80)
- **Minimum terminal size**: 80x24

## Animation

```rust
enum AnimationLevel { Full, Reduced, None }  // detects COLORTERM/TERM_PROGRAM
```

- Effects are built via tachyonfx (effects.rs)
- Transition constants (transitions.rs): Unlock→Main 1000ms, PageSwitch 500ms, Modal 200ms
- Note: tachyonfx 0.20.1 depends on ratatui 0.29.x; slide/expand effects temporarily use dissolve placeholders

## Keyboard Conventions

- **Main screen**: Tab cycles panel focus (Sidebar → List → Detail)
- **Focus Stack**: push_focus() when an overlay opens, pop_focus() when it closes
- **Visual Mode**: `v` enters multi-select, Space toggles selection, `a` selects all
- **Command routing**: keyboard events are dispatched based on the focused panel
