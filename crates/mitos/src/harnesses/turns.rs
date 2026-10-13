use std::path::Path;

use crate::domain::ThreadMode;

/// One headless exchange with a harness; `session` resumes a native session.
pub struct Turn<'a> {
    pub workdir: &'a Path,
    pub text: &'a str,
    pub session: Option<&'a str>,
    pub mode: ThreadMode,
    pub ephemeral: bool,
    /// A harness agent override; `None` follows the mode's default agent.
    pub agent: Option<&'a str>,
    /// A model reference, `provider/model`; `None` follows the harness default.
    pub model: Option<&'a str>,
}
