use serde_json::Value;

use crate::json::count_value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Counts {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

impl Counts {
    fn minus(self, other: Self) -> Self {
        Self {
            input: self.input.saturating_sub(other.input),
            output: self.output.saturating_sub(other.output),
            cached: self.cached.saturating_sub(other.cached),
        }
    }

    fn of(value: Option<&Value>) -> Option<Self> {
        let breakdown = value?.as_object()?;
        Some(Self {
            input: count_value(breakdown.get("inputTokens"))?,
            output: count_value(breakdown.get("outputTokens"))?,
            cached: count_value(breakdown.get("cachedInputTokens"))?,
        })
    }
}

/// Codex reports thread-lifetime token totals; Mitos wants what one turn
/// consumed. Whatever the thread had used before the turn is the baseline:
/// the last total seen before `begin()` (a resumed thread replays it), else
/// the first in-turn total minus its `last` request.
#[derive(Default)]
pub struct TurnTokens {
    began: bool,
    baseline: Option<Counts>,
}

impl TurnTokens {
    pub fn begin(&mut self) {
        self.began = true;
    }

    /// Token figures for this turn alone, or `None` when the baseline can't be
    /// known. Updates before `begin()` only set the baseline.
    pub fn consumed(&mut self, token_usage: &Value) -> Option<Counts> {
        let usage = token_usage.as_object()?;
        let total = Counts::of(usage.get("total"))?;
        if !self.began {
            self.baseline = Some(total);
            return None;
        }
        let baseline = if let Some(baseline) = self.baseline {
            baseline
        } else {
            let baseline = total.minus(Counts::of(usage.get("last"))?);
            self.baseline = Some(baseline);
            baseline
        };
        Some(total.minus(baseline))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn malformed_usage_is_ignored_and_counts_never_underflow() {
        assert!(Counts::of(Some(&json!({ "inputTokens": 1 }))).is_none());
        assert_eq!(
            Counts {
                input: 1,
                output: 2,
                cached: 3,
            }
            .minus(Counts {
                input: 2,
                output: 1,
                cached: 4,
            }),
            Counts {
                input: 0,
                output: 1,
                cached: 0,
            }
        );

        let mut tokens = TurnTokens::default();
        tokens.begin();
        assert!(tokens.consumed(&json!({ "total": {} })).is_none());
        assert!(
            tokens
                .consumed(&json!({
                    "total": { "inputTokens": 3, "outputTokens": 2, "cachedInputTokens": 1 }
                }))
                .is_none()
        );
    }
}
