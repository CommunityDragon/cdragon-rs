//! Helpers for parsing binary data
use thiserror::Error;

/// Type alias for `nom` result type commonly used
pub type IResult<I, O> = nom::IResult<I, O, ()>;

/// Helper macro to read and parse a buffer with nom
#[macro_export]
macro_rules! parse_buf {
    ($buf:expr, $parser:expr) => {{
        let result: nom::IResult<_, _, ()> = $parser.parse(&$buf[..]);
        let (_, parsed) = result.map_err($crate::parsing::ParseError::from)?;
        parsed
    }}
}


/// Parsing error
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum ParseError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unexpected data")]
    Error,
    #[error("too much data")]
    TooMuchData,
    #[error("not enough data")]
    NotEnoughData,
}

impl<T> From<nom::Err<T>> for ParseError {
    fn from(e: nom::Err<T>) -> Self {
        match e {
            nom::Err::Incomplete(_) => Self::NotEnoughData,
            nom::Err::Error(_) | nom::Err::Failure(_) => Self::Error,
        }
    }
}
