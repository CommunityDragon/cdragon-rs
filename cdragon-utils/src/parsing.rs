use std::fmt;

pub type IResult<I, O> = nom::IResult<I, O, ()>;

// `iresult.into()?` cannot infer error type to convert to
// Use `iresult.map_err(into_err)` instead
pub fn into_err(e: nom::Err<()>) -> ParseError {
    ParseError::from(e)
}

#[derive(Debug)]
pub enum ParseError {
    Error,
    TooMuchData,
    NotEnoughData,
    InvalidData(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            ParseError::Error => "unexpected data",
            ParseError::NotEnoughData => "not enough data",
            ParseError::TooMuchData => "too much data",
            ParseError::InvalidData(s) => s.as_str(),
        };
        f.write_str(s)
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl<T> From<nom::Err<T>> for ParseError {
    fn from(e: nom::Err<T>) -> Self {
        match e {
            nom::Err::Incomplete(_) => ParseError::NotEnoughData,
            nom::Err::Error(_) | nom::Err::Failure(_) => ParseError::Error,
        }
    }
}

