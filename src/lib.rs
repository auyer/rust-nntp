pub mod article;
pub mod codes;
mod connection;
pub mod errors;
pub mod newsgroup;
pub mod nntp_stream;

// re-export type for ease of use
pub use article::Article;
pub use codes::ResponseCode;
pub use errors::{NNTPError, Result};
pub use newsgroup::NewsGroup;
pub use nntp_stream::NNTPStream;
