use crate::domain::RequestId;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no such {0}")]
    NotFound(&'static str),
    #[error("request {0} is not pending")]
    RequestNotPending(RequestId),
}
