use std::io::{self, ErrorKind};
use std::result;
use thiserror::Error;

pub type Result<T> = result::Result<T, NNTPError>;

#[derive(Error, Debug)]
pub enum NNTPError {
    #[error("unknown data store error")]
    Unknown,
    #[error(transparent)]
    Io(#[from] io::Error),

    // TODO: move to response code mapping
    #[error("Server returned article unavailable (423) for this number")]
    ArticleUnavailable,
    #[error("Failed article with error: {error}")]
    FailedReadingArticle { error: io::Error },
    #[error("Failed reading response from stream. returned with error: {error}")]
    FailedReadingResponse { error: io::Error },

    #[error("Failed writing request to stream. returned with error: {error}")]
    FailedWritingRequest { error: io::Error },

    #[error("Failed Connecting. expeted: {expeted}, returned with error: {error}")]
    FailedConnecting {
        error: Box<NNTPError>,
        expeted: String,
    },
    #[error("Failed decoding body. Both UTF8 and WINDOWS_1252 failed. error")]
    DecodingError,

    #[error("Invalid Response froms server. Response: {response}")]
    InvalidResponse { response: String },

    #[error("Invalid message from server. likely reason: {reason} message: {message}")]
    InvalidMessage { message: String, reason: String },

    #[error("Invalid Response froms server. expeted {expeted}, received {received}")]
    ResponseCode { expeted: isize, received: isize },
}

pub fn check_network_error(error: NNTPError) -> bool {
    match error {
        NNTPError::Io(err) => {
            return check_io_network_error(&err);
        }
        _ => return false,
    }
}

fn check_io_network_error(err: &io::Error) -> bool {
    match err.kind() {
        ErrorKind::ConnectionRefused | // Connection actively refused by the peer
        ErrorKind::ConnectionReset |  // Connection reset by the peer
        ErrorKind::ConnectionAborted | // Connection aborted by the peer
        ErrorKind::BrokenPipe |       // Broken pipe (e.g., writing to a closed socket)
        ErrorKind::NotConnected |     // Not connected (e.g., trying to send on a disconnected socket)
        ErrorKind::TimedOut |         // Operation timed out
        ErrorKind::WouldBlock |       // Operation would block (non-blocking I/O)
        ErrorKind::Interrupted |      // Operation interrupted by a signal
        ErrorKind::UnexpectedEof      // Unexpected end of file (can indicate a closed connection)
        => true,
        _ => false,
    }
}

pub(crate) fn response_error_or_network(error: io::Error) -> NNTPError {
    if check_io_network_error(&error) {
        return NNTPError::Io(error);
    }
    NNTPError::FailedReadingResponse { error }
}

pub(crate) fn write_error_or_network(error: io::Error) -> NNTPError {
    if check_io_network_error(&error) {
        return NNTPError::Io(error);
    }
    NNTPError::FailedWritingRequest { error }
}

pub(crate) fn article_error_or_network(error: io::Error) -> NNTPError {
    if check_io_network_error(&error) {
        return NNTPError::Io(error);
    }
    NNTPError::FailedReadingArticle { error }
}
