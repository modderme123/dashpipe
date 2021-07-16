use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Halt Daemon")]
    HaltError,
}
