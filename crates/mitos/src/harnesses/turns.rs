use std::path::Path;

use crate::domain::ThreadMode;

/// One headless exchange with a harness; `session` resumes a native session.
pub struct Turn<'a> {
    pub workdir: &'a Path,
    pub text: &'a str,
    pub session: Option<&'a str>,
    pub mode: ThreadMode,
    pub ephemeral: bool,
}
