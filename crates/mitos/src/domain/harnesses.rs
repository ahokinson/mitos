string_enum! {
    enum HarnessKind("harness") {
        Claude => "claude",
        Codex => "codex",
        Hermes => "hermes",
        OpenCode => "opencode",
    }
}

impl HarnessKind {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Hermes, Self::OpenCode];
}

#[cfg(test)]
mod tests {
    use super::HarnessKind;

    #[test]
    fn every_harness_round_trips_through_its_name() {
        for harness in HarnessKind::ALL {
            assert_eq!(harness.as_str().parse::<HarnessKind>().unwrap(), harness);
        }
        assert!("bogus".parse::<HarnessKind>().is_err());
    }

    #[test]
    fn json_uses_the_same_names_as_the_cli_and_the_database() {
        for harness in HarnessKind::ALL {
            let json = serde_json::to_string(&harness).unwrap();
            assert_eq!(json, format!("\"{}\"", harness.as_str()));
            assert_eq!(serde_json::from_str::<HarnessKind>(&json).unwrap(), harness);
        }
    }

    #[test]
    fn display_honors_width_and_alignment() {
        assert_eq!(format!("{:<9}|", HarnessKind::Codex), "codex    |");
    }
}
