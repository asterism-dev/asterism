use std::time::Duration;

use asterism_proto::types::SessionStatus;

pub const DEFAULT_AFTER_MIN: u32 = 30;

/// `None` while hibernation is switched off.
pub fn timeout(minutes: Option<u32>, minute: Duration) -> Option<Duration> {
    match minutes.unwrap_or(DEFAULT_AFTER_MIN) {
        0 => None,
        n => Some(minute * n),
    }
}

/// Quiet output guards background work (subagents, monitors) that keeps running while hooks report idle.
pub fn should_hibernate(
    status: SessionStatus,
    quiet_for: Duration,
    attached: bool,
    has_agent_ref: bool,
    after: Option<Duration>,
) -> bool {
    status == SessionStatus::Idle
        && !attached
        && has_agent_ref
        && after.is_some_and(|after| quiet_for >= after)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    #[test]
    fn timeout_defaults_and_zero_disables() {
        assert_eq!(timeout(None, MIN), Some(MIN * 30));
        assert_eq!(timeout(Some(5), MIN), Some(MIN * 5));
        assert_eq!(timeout(Some(0), MIN), None);
    }

    #[test]
    fn only_quiet_idle_unattached_sessions_with_a_ref_hibernate() {
        let after = Some(MIN);
        assert!(should_hibernate(
            SessionStatus::Idle,
            MIN,
            false,
            true,
            after
        ));
        assert!(!should_hibernate(
            SessionStatus::Idle,
            MIN / 2,
            false,
            true,
            after
        ));
        assert!(!should_hibernate(
            SessionStatus::Idle,
            MIN,
            true,
            true,
            after
        ));
        assert!(!should_hibernate(
            SessionStatus::Idle,
            MIN,
            false,
            false,
            after
        ));
        assert!(!should_hibernate(
            SessionStatus::Idle,
            MIN,
            false,
            true,
            None
        ));
        for status in [
            SessionStatus::Working,
            SessionStatus::WaitingInput,
            SessionStatus::Exited,
            SessionStatus::Hibernated,
        ] {
            assert!(!should_hibernate(status, MIN, false, true, after));
        }
    }
}
