#[derive(Debug)]
pub enum Error {
    InvalidHeader(String),
    InvalidMethod(String),
    InvalidFormat,
    InvalidPacket,
    IOError(::std::io::Error),
    HyperError(::hyper::Error),
}

pub type Result<T> = ::std::result::Result<T, Error>;
