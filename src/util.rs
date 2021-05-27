use std::error::Error;
use quick_error::quick_error;

type BoxError = Box<dyn Error>;
pub type ResultB<T> = std::result::Result<T, BoxError>;

quick_error! {
    #[derive(Debug)]
    pub enum EE {
        MyError(msg: &'static str) {
            display("Error {}", msg)
        }
    }
}