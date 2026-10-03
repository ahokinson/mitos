use chrono::Utc;
use uuid::Uuid;

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn id() -> String {
    Uuid::new_v4().to_string()
}
