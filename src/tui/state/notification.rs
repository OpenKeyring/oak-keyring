//! Status bar notification state with priority queue and auto-dismiss.

/// Duration in ticks (1 tick ~ 50ms, so 40 ticks ~ 2s, 300 ticks ~ 15s).
const SUCCESS_TTL: u32 = 40; // 2s
const ERROR_TTL: u32 = 300; // 15s
const WARNING_TTL: u32 = 300; // 15s
const OPERATION_TTL: u32 = 0; // Until completed (no auto-expire)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagePriority {
    High,
    Medium,
    Low,
}

impl PartialOrd for MessagePriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MessagePriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let order = |p: &Self| match p {
            Self::High => 2,
            Self::Medium => 1,
            Self::Low => 0,
        };
        order(self).cmp(&order(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStyle {
    Success,
    Error,
    Warning,
    Operation,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub icon: &'static str,
    pub text: String,
    pub style: MessageStyle,
    pub ttl: u32,
    pub priority: MessagePriority,
}

impl StatusMessage {
    pub fn success(text: String) -> Self {
        Self {
            icon: "✓",
            text,
            style: MessageStyle::Success,
            ttl: SUCCESS_TTL,
            priority: MessagePriority::Low,
        }
    }

    pub fn error(text: String) -> Self {
        Self {
            icon: "✕",
            text,
            style: MessageStyle::Error,
            ttl: ERROR_TTL,
            priority: MessagePriority::High,
        }
    }

    pub fn warning(text: String) -> Self {
        Self {
            icon: "⚠",
            text,
            style: MessageStyle::Warning,
            ttl: WARNING_TTL,
            priority: MessagePriority::Medium,
        }
    }

    pub fn operation(text: String) -> Self {
        Self {
            icon: "…",
            text,
            style: MessageStyle::Operation,
            ttl: OPERATION_TTL,
            priority: MessagePriority::Medium,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self.style, MessageStyle::Success)
    }

    pub fn is_error(&self) -> bool {
        matches!(self.style, MessageStyle::Error)
    }
}

/// Priority rules: error > operation > success. New error preempts any current message.
/// New success waits behind active error. Operation blocks all until replaced.
#[derive(Debug, Default)]
pub struct NotificationState {
    pub current_message: Option<StatusMessage>,
    pub pending_message: Option<StatusMessage>,
    pub default_text: String,
}

impl NotificationState {
    pub fn enqueue(&mut self, msg: StatusMessage) {
        match &self.current_message {
            None => {
                self.current_message = Some(msg);
            }
            Some(current) => {
                if msg.priority <= current.priority {
                    self.pending_message = Some(msg);
                } else {
                    self.pending_message = self.current_message.take();
                    self.current_message = Some(msg);
                }
            }
        }
    }

    /// Call each tick to decrement TTL and expire messages.
    pub fn tick(&mut self) {
        if let Some(ref mut msg) = self.current_message {
            if msg.ttl > 0 {
                msg.ttl -= 1;
                if msg.ttl == 0 {
                    self.current_message = self.pending_message.take();
                }
            }
        } else if self.pending_message.is_some() {
            self.current_message = self.pending_message.take();
        }
    }

    pub fn clear(&mut self) {
        self.current_message = None;
        self.pending_message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_preempts_success() {
        let mut ns = NotificationState::default();
        ns.enqueue(StatusMessage::success("saved".into()));
        assert!(ns.current_message.as_ref().unwrap().is_success());
        ns.enqueue(StatusMessage::error("failed".into()));
        assert!(ns.current_message.as_ref().unwrap().is_error());
    }

    #[test]
    fn tick_expires_message() {
        let mut ns = NotificationState::default();
        ns.enqueue(StatusMessage::success("ok".into()));
        for _ in 0..39 {
            ns.tick();
            assert!(ns.current_message.is_some());
        }
        ns.tick();
        assert!(ns.current_message.is_none());
    }

    #[test]
    fn operation_does_not_expire() {
        let mut ns = NotificationState::default();
        ns.enqueue(StatusMessage::operation("syncing".into()));
        for _ in 0..200 {
            ns.tick();
        }
        assert!(ns.current_message.is_some());
    }

    #[test]
    fn pending_promoted_after_expiry() {
        let mut ns = NotificationState::default();
        ns.enqueue(StatusMessage::success("ok".into()));
        ns.enqueue(StatusMessage::error("err".into()));
        // Error preempts success, success goes to pending
        assert!(ns.current_message.as_ref().unwrap().is_error());
        // Expire the error
        for _ in 0..300 {
            ns.tick();
        }
        // Pending success should now be current
        assert!(ns.current_message.is_some());
    }

    #[test]
    fn clear_removes_all() {
        let mut ns = NotificationState::default();
        ns.enqueue(StatusMessage::error("err".into()));
        ns.enqueue(StatusMessage::success("ok".into()));
        ns.clear();
        assert!(ns.current_message.is_none());
        assert!(ns.pending_message.is_none());
    }
}
