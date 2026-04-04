//! # nntp
//!
//! A NNTP (Network News Transfer Protocol) client library for Rust, implementing
//! [RFC 3977](https://tools.ietf.org/html/rfc3977) and
//! [RFC 4643](https://tools.ietf.org/html/rfc4643) (authentication extension).
//!
//! ## Features
//!
//! - Connect to NNTP servers with automatic retry and exponential backoff
//! - Retrieve articles by number or message ID
//! - Fetch article headers, body, or full content
//! - List and select newsgroups
//! - Post messages to newsgroups
//! - USER/PASS authentication with automatic re-authentication on reconnect
//! - UTF-8 and WINDOWS-1252 encoding support
//!
//! ## Quick Start
//!
//! ```no_run
//! use nntp::NNTPStream;
//!
//! let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
//!     .expect("Failed to connect");
//!
//! // Select a newsgroup
//! let group = client.group("comp.test")
//!     .expect("Failed to select group");
//! println!("Selected: {} ({} articles)", group.name, group.number);
//!
//! // Fetch an article
//! match client.article_by_number(1) {
//!     Ok(article) => {
//!         for (key, value) in &article.headers {
//!             println!("{}: {}", key, value);
//!         }
//!     }
//!     Err(e) => eprintln!("Failed to fetch article: {}", e),
//! }
//!
//! // Disconnect
//! let _ = client.quit();
//! ```

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
