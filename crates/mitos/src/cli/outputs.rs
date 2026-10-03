use anyhow::Result;
use serde::Serialize;

pub fn emit<T: Serialize>(json: bool, value: T, text: impl FnOnce(T)) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        text(value);
    }
    Ok(())
}
