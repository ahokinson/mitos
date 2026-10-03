use super::*;

#[test]
fn thread_status_round_trips_through_its_string_form() {
    for status in [
        ThreadStatus::Active,
        ThreadStatus::Paused,
        ThreadStatus::Archived,
    ] {
        assert_eq!(status.as_str().parse::<ThreadStatus>().unwrap(), status);
    }
}

#[test]
fn event_kind_round_trips_through_its_string_form() {
    for kind in [
        EventKind::UserMessage,
        EventKind::AssistantMessage,
        EventKind::AssistantDelta,
        EventKind::ToolCall,
        EventKind::ToolResult,
        EventKind::Status,
        EventKind::Usage,
        EventKind::Error,
        EventKind::Note,
        EventKind::Decision,
        EventKind::Question,
        EventKind::RequestOpened,
        EventKind::RequestAnswered,
        EventKind::ModeChanged,
        EventKind::ThreadCreated,
        EventKind::HarnessBound,
        EventKind::HarnessUnbound,
        EventKind::HandoffCarryover,
    ] {
        assert_eq!(kind.as_str().parse::<EventKind>().unwrap(), kind);
    }
}

#[test]
fn thread_mode_round_trips_through_its_string_form() {
    for mode in [ThreadMode::Plan, ThreadMode::Build] {
        assert_eq!(mode.as_str().parse::<ThreadMode>().unwrap(), mode);
    }
}

#[test]
fn thread_mode_defaults_to_build() {
    assert_eq!(ThreadMode::default(), ThreadMode::Build);
}

#[test]
fn unknown_values_name_the_enum_in_the_error() {
    let error = "nope".parse::<RequestKind>().unwrap_err();
    assert_eq!(error.to_string(), "unknown request kind \"nope\"");
}

#[test]
fn request_enums_round_trip_through_their_string_forms() {
    for kind in [
        RequestKind::Question,
        RequestKind::Permission,
        RequestKind::PlanApproval,
    ] {
        assert_eq!(kind.as_str().parse::<RequestKind>().unwrap(), kind);
    }
    for status in [
        RequestStatus::Pending,
        RequestStatus::Answered,
        RequestStatus::Cancelled,
    ] {
        assert_eq!(status.as_str().parse::<RequestStatus>().unwrap(), status);
    }
}
