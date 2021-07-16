use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("halt")]
    HaltError ,
}
