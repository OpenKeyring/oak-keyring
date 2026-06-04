Output demo.gif

Require vhs

Set FontFamily "FiraCode Nerd Font Mono"
Set FontSize 15
Set Padding 20
Set Width 1920
Set Height 1248
Set TypingSpeed 50ms

# Launch oak-keyring (clean short command via wrapper script)
Type "./demo-run"
Enter

# Wait for app to start
Sleep 5s

# ============================================================
# Welcome Screen — switch to English, then select "Create New"
# ============================================================

Type "l"
Sleep 500ms
Enter
Sleep 1s

# ============================================================
# Recovery Display — 24-word mnemonic
# ============================================================

# Pause for viewer to read the words
Sleep 2s

# Navigate to checkbox: Copy → Regenerate → LearnMore → Checkbox
Tab
Sleep 150ms
Tab
Sleep 150ms
Tab
Sleep 300ms

# Check "I have safely stored my recovery key"
Space
Sleep 500ms

# Proceed to verification
Enter
Sleep 1s

# ============================================================
# Recovery Verify
# Positions [2, 7, 14, 21] → tenant, coin, asthma, miss
# ============================================================

Type "tenant"
Sleep 200ms
Tab
Sleep 200ms

Type "coin"
Sleep 200ms
Tab
Sleep 200ms

Type "asthma"
Sleep 200ms
Tab
Sleep 200ms

Type "miss"
Sleep 300ms

Enter
Sleep 1s

# ============================================================
# Set Password intro → Enter to open SetNewMasterPassword screen
# ============================================================

Enter
Sleep 1s

# ============================================================
# SetNewMasterPassword — type and confirm master password
# ============================================================

Type "OakDemo2024"
Sleep 1s
Tab
Sleep 1s
Type "OakDemo2024"
Sleep 1s

Enter
Sleep 3s

# ============================================================
# Main Screen — open create entry dialog
# ============================================================

Type "n"
Sleep 1s

# ============================================================
# Create Record — Google account (Login type, 8 fields)
# ============================================================

# Field 0: Credential Type — Login (default), skip
Tab
Sleep 200ms

# Field 1: Name
Type "Google"
Sleep 300ms
Tab
Sleep 200ms

# Field 2: URL
Type "google.com"
Sleep 300ms
Tab
Sleep 200ms

# Field 3: Username
Type "user@gmail.com"
Sleep 300ms
Tab
Sleep 200ms

# Field 4: Password (strong password)
Type "G00gle@Pass2024"
Sleep 300ms
Tab
Sleep 200ms

# Field 5: Expiry — skip
Tab
Sleep 200ms

# Field 6: Tags — add "work" tag
Type "work"
Sleep 300ms
Enter
Sleep 300ms
Tab
Sleep 200ms

# Field 7: Notes — skip, Tab to Save button
Tab
Sleep 500ms

# Save the entry
Enter
Sleep 2s

# ============================================================
# Main Screen — navigate panels
# ============================================================

Tab
Sleep 800ms
Tab
Sleep 800ms
Down
Sleep 500ms
Up
Sleep 500ms

# ============================================================
# Create second entry — show password generator
# ============================================================

Type "n"
Sleep 1s

# Field 0: Type — skip
Tab
Sleep 200ms

# Field 1: Name
Type "GitHub"
Sleep 300ms
Tab
Sleep 200ms

# Field 2: URL
Type "github.com"
Sleep 300ms
Tab
Sleep 200ms

# Field 3: Username — skip
Tab
Sleep 200ms

# Field 4: Password — use generator
Ctrl+g
Sleep 2s

# Regenerate to show the feature
Type "r"
Sleep 1s

# Accept the generated password
Type "y"
Sleep 300ms
Escape
Sleep 500ms

# Skip to Save button
Tab
Sleep 100ms
Tab
Sleep 100ms
Tab
Sleep 100ms
Tab
Sleep 500ms

# Save
Enter
Sleep 2s
