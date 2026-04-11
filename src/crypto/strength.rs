const SPECIAL_CHARS: &str = "!#$*+-=?@^_~";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrengthLevel {
    VeryWeak,
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

impl StrengthLevel {
    pub fn label_zh(self) -> &'static str {
        match self {
            StrengthLevel::VeryWeak => "极弱",
            StrengthLevel::Weak => "弱",
            StrengthLevel::Fair => "中等",
            StrengthLevel::Strong => "强",
            StrengthLevel::VeryStrong => "极强",
        }
    }

    pub fn color_hex(self) -> &'static str {
        match self {
            StrengthLevel::VeryWeak => "#f7768e",
            StrengthLevel::Weak => "#ff9e64",
            StrengthLevel::Fair => "#e0af68",
            StrengthLevel::Strong => "#9ece6a",
            StrengthLevel::VeryStrong => "#73daca",
        }
    }
}

#[derive(Debug)]
pub struct PasswordStrength {
    pub level: StrengthLevel,
    pub char_types: u8,
    pub bar_fill: u8,
}

pub fn evaluate_strength(password: &str) -> PasswordStrength {
    let len = password.len();
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| SPECIAL_CHARS.contains(c));

    let char_types = [has_lower, has_upper, has_digit, has_special]
        .iter()
        .filter(|&&b| b)
        .count() as u8;

    let (level, bar_fill) = if len < 8 || char_types <= 1 {
        (StrengthLevel::VeryWeak, 3u8)
    } else if len <= 11 && char_types <= 2 {
        (StrengthLevel::Weak, 6)
    } else if len <= 15 && char_types == 3 {
        (StrengthLevel::Fair, 9)
    } else if len <= 23 && char_types == 4 {
        (StrengthLevel::Strong, 12)
    } else if len >= 24 && char_types == 4 {
        (StrengthLevel::VeryStrong, 16)
    } else {
        (StrengthLevel::Weak, 6)
    };

    PasswordStrength {
        level,
        char_types,
        bar_fill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_very_weak_short() {
        let result = evaluate_strength("a");
        assert_eq!(result.level, StrengthLevel::VeryWeak);
        assert_eq!(result.bar_fill, 3);
    }

    #[test]
    fn test_very_weak_one_type() {
        let result = evaluate_strength("abcdefgh");
        assert_eq!(result.level, StrengthLevel::VeryWeak);
        assert_eq!(result.bar_fill, 3);
    }

    #[test]
    fn test_weak_8_chars_2_types() {
        let result = evaluate_strength("abcd1234");
        assert_eq!(result.level, StrengthLevel::Weak);
        assert_eq!(result.bar_fill, 6);
    }

    #[test]
    fn test_weak_11_chars_2_types() {
        let result = evaluate_strength("abcd1234abc");
        assert_eq!(result.level, StrengthLevel::Weak);
        assert_eq!(result.bar_fill, 6);
    }

    #[test]
    fn test_fair_12_chars_3_types() {
        let result = evaluate_strength("abcd1234ABCD");
        assert_eq!(result.level, StrengthLevel::Fair);
        assert_eq!(result.bar_fill, 9);
    }

    #[test]
    fn test_strong_16_chars_4_types() {
        let result = evaluate_strength("abcd1234ABCD!@ab");
        assert_eq!(result.level, StrengthLevel::Strong);
        assert_eq!(result.bar_fill, 12);
    }

    #[test]
    fn test_very_strong_24_chars() {
        let result = evaluate_strength("abcd1234ABCD!@ababcd1234");
        assert_eq!(result.level, StrengthLevel::VeryStrong);
        assert_eq!(result.bar_fill, 16);
    }

    #[test]
    fn test_fallback_to_weak() {
        let result = evaluate_strength("abcd1234abcd");
        assert_eq!(result.level, StrengthLevel::Weak);
        assert_eq!(result.bar_fill, 6);
    }

    #[test]
    fn test_label_zh() {
        assert_eq!(StrengthLevel::VeryWeak.label_zh(), "极弱");
        assert_eq!(StrengthLevel::Weak.label_zh(), "弱");
        assert_eq!(StrengthLevel::Fair.label_zh(), "中等");
        assert_eq!(StrengthLevel::Strong.label_zh(), "强");
        assert_eq!(StrengthLevel::VeryStrong.label_zh(), "极强");
    }

    #[test]
    fn test_color_hex() {
        assert_eq!(StrengthLevel::VeryWeak.color_hex(), "#f7768e");
        assert_eq!(StrengthLevel::Weak.color_hex(), "#ff9e64");
        assert_eq!(StrengthLevel::Fair.color_hex(), "#e0af68");
        assert_eq!(StrengthLevel::Strong.color_hex(), "#9ece6a");
        assert_eq!(StrengthLevel::VeryStrong.color_hex(), "#73daca");
    }

    #[test]
    fn test_special_char_detection_matches_password_spec() {
        for c in "!#$*+-=?@^_~".chars() {
            let pw = format!("abcdABCD1234{c}");
            let result = evaluate_strength(&pw);
            assert_eq!(result.char_types, 4, "special char '{c}' not detected");
        }

        assert_eq!(evaluate_strength("abcdABCD1234@").char_types, 4);

        let result = evaluate_strength("abcdABCD1234");
        assert_eq!(result.char_types, 3);
    }
}
