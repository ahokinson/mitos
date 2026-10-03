use std::path::Path;

use anyhow::Result;

use crate::wire::responses::LaunchPlan;

pub trait HarnessRunner {
    fn run(&self, launch: &LaunchPlan, workdir: &Path) -> Result<RunStatus>;
}

#[derive(Clone, Debug)]
pub struct RunStatus {
    pub success: bool,
    pub description: String,
}
