use quick_error::quick_error;
use std::error::Error;

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

pub fn box_error<T: Error + 'static>(e: T) -> BoxError {
    e.into()
}
