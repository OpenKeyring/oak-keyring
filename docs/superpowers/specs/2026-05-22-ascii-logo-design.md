# ASCII Logo for Onboarding Welcome Screen

## Problem

Onboarding welcome screen displays product name "OpenKeyring" as single-line text (`🔐 OpenKeyring`), which looks small and lacks visual impact.

## Solution

Replace the brand text with a Slant-style ASCII art logo on wide terminals, with graceful degradation to the existing text on narrow terminals.

## Design

### ASCII Logo

Slant font rendering of "OpenKeyring" (77 columns wide, 6 lines):

```
   ____                   __ __                _
  / __ \____  ___  ____  / //_/__  __  _______(_)___  ____ _
 / / / / __ \/ _ \/ __ \/ ,< / _ \/ / / / ___/ / __ \/ __ `/
/ /_/ / /_/ /  __/ / / / /| /  __/ /_/ / /  / / / / / /_/ /
\____/ .___/\___/_/ /_/_/ |_\___/\__, /_/  /_/_/ /_/\__, /
    /_/                         /____/             /____/
```

Gradient colors from brand purple to deeper purple, one color per line.

### Responsive Behavior

- **Terminal width >= 80**: Render ASCII logo (6 lines), remove separator line
- **Terminal width < 80**: Keep existing `🔐 OpenKeyring` single-line text

Uses existing `BREAKPOINT_MINIMUM = 80` from `terminal.rs`.

### Layout Changes (welcome view only)

- Content width: `Max(60)` -> `Max(80)`
- Content height: 21 -> 25 (wide terminal) or 21 (narrow terminal)
- Separator line removed when logo is shown

## File Changes

| File | Change |
|------|--------|
| `src/tui/screens/onboarding/logo.rs` | **New** -- ASCII art constants + gradient-colored `ascii_logo()` function |
| `src/tui/screens/onboarding/mod.rs` | Declare `mod logo` |
| `src/tui/screens/onboarding/views_setup.rs` | Conditional logo/text rendering, adjusted layout constraints |
| `src/tui/screens/onboarding/tests.rs` | Update welcome screen snapshot tests |

## Out of Scope

- ASCII logo on other screens (unlock, about) -- welcome only
- Runtime ASCII art generation (figlet-rs etc.)
- Animation effects on the logo
