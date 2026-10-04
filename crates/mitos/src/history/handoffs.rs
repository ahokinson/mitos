use serde_json::Value;

use super::messages::{ASSISTANT, is_handoff_preamble};
use crate::wire::responses::{
    CollectedHandoff, CollectedMessage, CollectedTranscript, CollectedUsage,
};

pub fn empty_handoff(native_session: Option<String>) -> CollectedHandoff {
    CollectedHandoff::new(native_session.map(Value::String), None, None)
}

pub fn transcript_handoff(
    session_id: String,
    all_messages: Vec<CollectedMessage>,
    usage: Option<CollectedUsage>,
) -> CollectedHandoff {
    let messages: Vec<CollectedMessage> = all_messages
        .into_iter()
        .filter(|message| !is_handoff_preamble(message))
        .collect();
    let turns = u32::try_from(
        messages
            .iter()
            .filter(|message| message.role == ASSISTANT)
            .count(),
    )
    .unwrap_or(u32::MAX);
    let usage = match usage {
        Some(usage) => Some(CollectedUsage {
            turns: Some(turns),
            ..usage
        }),
        None if turns > 0 => Some(CollectedUsage {
            turns: Some(turns),
            ..CollectedUsage::default()
        }),
        None => None,
    };
    CollectedHandoff::new(
        Some(Value::String(session_id)),
        Some(CollectedTranscript { messages }),
        usage,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::PREAMBLE_PREFIX;

    fn message(role: &str, text: &str) -> CollectedMessage {
        CollectedMessage {
            role: role.into(),
            text: text.into(),
        }
    }

    #[test]
    fn a_transcript_drops_the_prompt_mitos_sent_to_catch_the_harness_up() {
        let handoff = transcript_handoff(
            "s1".into(),
            vec![
                message(
                    "user",
                    &format!("{PREAMBLE_PREFIX}abc. The conversation so far follows"),
                ),
                message("user", "Hey, who are you?"),
                message("assistant", "Hello."),
            ],
            None,
        );
        let transcript = handoff.transcript.unwrap();
        let texts: Vec<_> = transcript
            .messages
            .iter()
            .map(|m| m.text.as_str())
            .collect();
        assert_eq!(texts, ["Hey, who are you?", "Hello."]);
        assert_eq!(handoff.usage.unwrap().turns, Some(1));
    }

    #[test]
    fn turns_are_added_to_given_usage_and_absent_when_there_are_none() {
        let usage = CollectedUsage {
            input_tokens: Some(5),
            ..CollectedUsage::default()
        };
        let with = transcript_handoff("s".into(), vec![message("assistant", "a")], Some(usage));
        assert_eq!(with.usage.as_ref().unwrap().input_tokens, Some(5));
        assert_eq!(with.usage.unwrap().turns, Some(1));
        let none = transcript_handoff("s".into(), vec![message("user", "q")], None);
        assert!(none.usage.is_none());
    }
}
