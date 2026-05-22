# RecoveryDisplay UX Improvement

## Problem

The onboarding RecoveryDisplay step lacks guidance for ordinary users:

- No explanation of what a recovery key is or why it matters
- No instruction to record words in order
- No guidance on how/where to save the key
- "Check the box above to continue" text (`TEXT_MUTED`) is barely visible

## Solution

Two independent changes applied to the RecoveryDisplay view.

### 1. Add instruction line + collapsible "Learn more"

**Instruction line** (always visible, 1 row):

Centered text in `TEXT_SECONDARY`:

- EN: "This is the only way to recover your vault. Write down in order."
- ZH: "这是恢复 vault 的唯一方式，请按顺序抄写保存。"

Inserted between title and word grid.

**Collapsible "Learn more" toggle** (1 row collapsed, 4 rows expanded):

Default state: collapsed, shows `▶ Learn more` in `TEXT_MUTED`.

Expanded content (3 lines in `TEXT_SECONDARY`):

- EN:
  - "Your recovery key consists of 24 words."
  - "Write them down on paper in the exact order shown."
  - "Store offline in a safe place. Never screenshot or save to cloud."
- ZH:
  - "恢复密钥由 24 个单词组成，是恢复 vault 数据的唯一方式。"
  - "请按显示的顺序抄写到纸上，顺序不能颠倒。"
  - "保存在安全的离线位置，不要截图或存到云端。"

Toggle behavior: Enter or Space when focused toggles expand/collapse.

**Layout change**: Remove 2 spacer rows from the word grid (between groups of 2 word rows) to free space. Grid height: 12 → 10 rows. Net content height unchanged when collapsed.

**New focus target**: Add `RecoveryFocus::LearnMoreToggle` to the focus cycle (between RegenerateButton and ConfirmCheckbox).

### 2. Checkbox prompt color

Change `check_box_to_continue` text color from `TEXT_MUTED` to `TEXT_SECONDARY`.

## Files to modify

| File | Change |
|------|--------|
| `views_recovery.rs` | Add instruction line, learn-more toggle, adjust grid height, adjust row indices |
| `types.rs` | Add `LearnMoreToggle` to `RecoveryFocus` enum |
| `screen.rs` | Add `learn_more_expanded: bool` field, reset in `on_mount`/`on_unmount` |
| `handlers.rs` (or handler module) | Handle focus navigation and toggle for LearnMoreToggle |
| `locales/en.yml` | Add new i18n keys |
| `locales/zh-CN.yml` | Add new i18n keys |
| Snapshot tests | Update affected snapshots |

## New i18n keys

| Key | EN | ZH |
|-----|----|----|
| `tui.entry.recovery_key_instruction` | This is the only way to recover your vault. Write down in order. | 这是恢复 vault 的唯一方式，请按顺序抄写保存。 |
| `tui.entry.recovery_learn_more_collapsed` | ▶ Learn more | ▶ 了解更多 |
| `tui.entry.recovery_learn_more_expanded` | ▼ Learn more | ▼ 了解更多 |
| `tui.entry.recovery_learn_more_l1` | Your recovery key consists of 24 words. | 恢复密钥由 24 个单词组成，是恢复 vault 数据的唯一方式。 |
| `tui.entry.recovery_learn_more_l2` | Write them down on paper in the exact order shown. | 请按显示的顺序抄写到纸上，顺序不能颠倒。 |
| `tui.entry.recovery_learn_more_l3` | Store offline in a safe place. Never screenshot or save to cloud. | 保存在安全的离线位置，不要截图或存到云端。 |

## Layout (collapsed, wide mode)

```
rows[0]  hdr (logo or brand)          — 4 rows
rows[1]  title                        — 1 row
rows[2]  instruction                  — 1 row   ← NEW
rows[3]  word grid                    — 10 rows (was 12)
rows[4]  buttons row                  — 1 row
rows[5]  clipboard warning            — 1 row (conditional)
rows[6]  learn more toggle            — 1 row   ← NEW
rows[7]  checkbox                     — 1 row
rows[8]  next step / instruction      — 1 row
rows[9]  hint                         — 1 row
rows[10] step indicator               — 1 row
Total: hdr + 18 = 22 rows (fits 80x24 with 2 rows padding)
```

## Layout (expanded, wide mode)

```
rows[0]  hdr (logo or brand)          — 4 rows
rows[1]  title                        — 1 row
rows[2]  instruction                  — 1 row
rows[3]  word grid                    — 10 rows
rows[4]  buttons row                  — 1 row
rows[5]  clipboard warning            — 1 row (conditional)
rows[6]  learn more toggle (▼)        — 1 row
rows[7]  learn more line 1            — 1 row   ← NEW
rows[8]  learn more line 2            — 1 row   ← NEW
rows[9]  learn more line 3            — 1 row   ← NEW
rows[10] checkbox                     — 1 row
rows[11] next step / instruction      — 1 row
rows[12] hint                         — 1 row
rows[13] step indicator               — 1 row
Total: hdr + 21 = 25 rows (exceeds 80x24 by 1 row, content flush to edges)
```

On 80x24 the expanded state fills the terminal with no vertical padding. On larger terminals content remains centered.
