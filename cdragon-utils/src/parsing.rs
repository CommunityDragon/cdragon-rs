//! Helpers for parsing binary data
use thiserror::Error;

/// Type alias for `nom` result type commonly used
pub type IResult<I, O, E = ()> = nom::IResult<I, O, E>;

/// Helper macro to read and parse a buffer with nom
#[macro_export]
macro_rules! parse_buf {
    ($err:ty, $buf:expr, $parser:expr) => {{
        let result: nom::IResult<_, _, $err> = $parser.parse(&$buf[..]);
        let (_, parsed) = result.map_err($crate::parsing::ParseError::from)?;
        parsed
    }};
    ($buf:expr, $parser:expr) => { parse_buf!((), $buf, $parser) };
}

/// Helper trait to read a known fix length as an array
pub trait ReadArray {
    /// Read into a fixed-size array
    ///
    /// To be replaced by `Read::read_array()` when stabilized.
    fn read_array_n<const N: usize>(&mut self) -> std::io::Result<[u8; N]>;
}

impl<R: std::io::Read> ReadArray for R {
    fn read_array_n<const N: usize>(&mut self) -> std::io::Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}


/// Parsing error
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum ParseError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unexpected data")]
    UnexpectedData,
    #[error("unexpected data: {0}")]
    Error(String),
    #[error("too much data")]
    TooMuchData,
    #[error("not enough data")]
    NotEnoughData,
}

impl<E: std::fmt::Debug> From<nom::Err<E>> for ParseError {
    fn from(e: nom::Err<E>) -> Self {
        match e {
            nom::Err::Incomplete(_) => Self::NotEnoughData,
            nom::Err::Error(e) => Self::Error(format!("{:?}", e)),
            nom::Err::Failure(e) => Self::Error(format!("{:?}", e)),
        }
    }
}

