#[derive(Debug, thiserror::Error)]
#[error("unknown {label} {value:?}")]
pub struct ParseError {
    pub label: &'static str,
    pub value: String,
}
