use std::io::{Read, Write};
use std::net::TcpStream;
use std::string::String;
use std::vec::Vec;

use rustls::{ClientConnection, StreamOwned};

use crate::address::ServerAddress;
use crate::article::Article;
use crate::codes::{self, ResponseCode};
use crate::connection::connect_with_retry;
use crate::errors::{self, NNTPError, Result};
use crate::newsgroup::NewsGroup;
use crate::tls::wrap_tls;

/// The underlying stream type — either plain TCP or TLS-wrapped.
enum InnerStream {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for InnerStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            InnerStream::Plain(s) => s.read(buf),
            InnerStream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for InnerStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            InnerStream::Plain(s) => s.write(buf),
            InnerStream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            InnerStream::Plain(s) => s.flush(),
            InnerStream::Tls(s) => s.flush(),
        }
    }
}

/// A connection to an NNTP server.
///
/// `NNTPStream` wraps a TCP connection (optionally TLS-encrypted) and provides
/// methods for sending NNTP commands and parsing responses. It handles encoding
/// detection (UTF-8 and WINDOWS-1252), automatic reconnection with retry, and
/// optional authentication.
///
/// # TLS Support
///
/// TLS is automatically enabled when:
/// - The address uses the `nntps://` scheme
/// - The port is 563 (the standard NNTPS port)
///
/// For explicit TLS control, use [`NNTPStream::connect_with`] with a
/// [`ServerAddress`] configured via [`ServerAddress::with_tls`].
///
/// # Example
///
/// ```no_run
/// use nntp::NNTPStream;
///
/// // Plain connection on port 119
/// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
///     .expect("Failed to connect");
///
/// // TLS connection on port 563 (auto-enabled)
/// let mut client_tls = NNTPStream::connect("nntp.example.com:563".to_string())
///     .expect("Failed to connect");
///
/// let _ = client.quit();
/// ```
pub struct NNTPStream {
    server_addr: ServerAddress,
    stream: InnerStream,
    authenticated: bool,
    username: Option<String>,
    password: Option<String>,
}

/// Connection management
impl NNTPStream {
    /// Connects to an NNTP server at the given address.
    ///
    /// The address can be in several formats:
    /// - `"host:port"` — TLS auto-enabled if port is 563
    /// - `"nntp://host"` — plain connection on port 119
    /// - `"nntp://host:port"` — plain connection on specified port
    /// - `"nntps://host"` — TLS connection on port 563
    /// - `"nntps://host:port"` — TLS connection on specified port
    ///
    /// The connection attempt uses exponential backoff with up to 3 retries.
    ///
    /// On successful connection, the server's greeting response (code 200 or 201)
    /// is consumed and logged.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::FailedConnecting`] if the connection fails or the
    /// server greeting is not recognized.
    /// Returns [`NNTPError::TlsError`] if the TLS handshake fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    /// ```
    pub fn connect(addr: String) -> Result<NNTPStream> {
        let server_addr = ServerAddress::parse(&addr).map_err(|e| NNTPError::FailedConnecting {
            expected: "valid address".to_owned(),
            error: Box::new(NNTPError::TlsError {
                message: e.to_string(),
            }),
        })?;
        Self::establish_connection(server_addr)
    }

    /// Connects to an NNTP server using an explicit [`ServerAddress`].
    ///
    /// This method gives full control over TLS configuration.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::{NNTPStream, ServerAddress, TlsConfig};
    ///
    /// // Connect with TLS, skipping certificate validation (DANGEROUS)
    /// let addr = ServerAddress::with_tls(
    ///     "nntp.example.com",
    ///     563,
    ///     TlsConfig { danger_accept_invalid_certs: true },
    /// );
    /// let client = NNTPStream::connect_with(addr);
    /// ```
    pub fn connect_with(server_addr: ServerAddress) -> Result<NNTPStream> {
        Self::establish_connection(server_addr)
    }

    /// Internal: establishes TCP connection and optional TLS handshake
    fn establish_connection(server_addr: ServerAddress) -> Result<NNTPStream> {
        let addr_str = format!("{}:{}", server_addr.host, server_addr.port);
        let tcp_stream = connect_with_retry(&addr_str, 3, 7_0000, 100)?;

        let stream = if server_addr.tls.is_some() {
            let tls_stream =
                wrap_tls(tcp_stream, &server_addr).map_err(|e| NNTPError::FailedConnecting {
                    expected: "TLS handshake".to_owned(),
                    error: Box::new(NNTPError::TlsError {
                        message: e.to_string(),
                    }),
                })?;
            InnerStream::Tls(Box::new(tls_stream))
        } else {
            InnerStream::Plain(tcp_stream)
        };

        let mut socket = NNTPStream {
            stream,
            server_addr,
            authenticated: false,
            username: None,
            password: None,
        };

        match socket.read_response(vec![
            ResponseCode::ServiceAvailablePostingAllowed,
            ResponseCode::ServiceAvailablePostingProhibited,
        ]) {
            Ok((status, response)) => log::info!("Connect: {} {}", status, response),
            Err(err) => {
                return Err(NNTPError::FailedConnecting {
                    expected: "greeting response".to_owned(),
                    error: Box::new(err),
                });
            }
        }

        Ok(socket)
    }

    /// Reconnects to the server using the same address and TLS configuration.
    ///
    /// This is useful after a connection has been lost. If the stream was
    /// previously authenticated, this method will automatically re-authenticate
    /// using the stored credentials.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::FailedConnecting`] if reconnection fails, or
    /// propagates authentication errors from [`NNTPStream::user_password_authenticate`].
    pub fn re_connect(&mut self) -> Result<()> {
        let addr_str = format!("{}:{}", self.server_addr.host, self.server_addr.port);
        let tcp_stream = connect_with_retry(&addr_str, 3, 7_000, 100)?;

        self.stream = if self.server_addr.tls.is_some() {
            let tls_stream = wrap_tls(tcp_stream, &self.server_addr).map_err(|e| {
                NNTPError::FailedConnecting {
                    expected: "TLS handshake".to_owned(),
                    error: Box::new(NNTPError::TlsError {
                        message: e.to_string(),
                    }),
                }
            })?;
            InnerStream::Tls(Box::new(tls_stream))
        } else {
            InnerStream::Plain(tcp_stream)
        };

        let res = match self.read_response(vec![
            ResponseCode::ServiceAvailablePostingAllowed,
            ResponseCode::ServiceAvailablePostingProhibited,
        ]) {
            Ok((status, response)) => {
                log::info!("Connect: {} {}", status, response);
                Ok(())
            }
            Err(err) => Err(NNTPError::FailedConnecting {
                expected: "greeting response".to_owned(),
                error: Box::new(err),
            }),
        };

        // if the server was authenticated, re-auth after reconnection
        if self.authenticated {
            self.authenticated = false;
            if let (Some(username), Some(password)) = (self.username.clone(), self.password.clone())
                && let Err(e) = self.user_password_authenticate(&username, &password)
            {
                log::warn!("Re-authentication after reconnect failed: {}", e);
                return Err(e);
            }
        }

        res
    }

    /// Authenticates with the server using the `AUTHINFO USER/PASS` method (RFC 4643).
    ///
    /// Sends `AUTHINFO USER <username>` followed by `AUTHINFO PASS <password>`.
    /// Some servers may accept the USER command alone (returning 281), in which
    /// case the PASS step is skipped.
    ///
    /// This method also issues `MODE READER` before authentication, as some
    /// servers require it. Credentials are stored internally so that
    /// [`NNTPStream::re_connect`] can re-authenticate automatically.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ResponseCode`] if authentication fails (e.g. wrong
    /// credentials), or propagates I/O errors.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    /// client.user_password_authenticate("user", "password")
    ///     .expect("Authentication failed");
    /// ```
    pub fn user_password_authenticate(&mut self, username: &str, password: &str) -> Result<()> {
        // TODO: allow posting mode too

        let user_response = self.auth_user(username)?;

        // If the server already accepted authentication with USER alone, skip PASS
        if user_response.starts_with("281") {
            self.authenticated = true;
            self.username = Some(username.to_owned());
            self.password = Some(password.to_owned());
            return Ok(());
        }

        // Server responded with 381 (Password Required), send PASS
        self.auth_password(password)?;

        self.authenticated = true;
        self.username = Some(username.to_owned());
        self.password = Some(password.to_owned());
        Ok(())
    }
}

/// Article retrieval commands (RFC 3977 §6)
impl NNTPStream {
    /// Retrieves the full article (headers and body) indicated by the current
    /// article number in the currently selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists at the
    /// current number, or a response error with code 412 if no group is selected.
    pub fn article(&mut self) -> Result<Article> {
        self.retrieve_article("ARTICLE\r\n")
    }

    /// Retrieves the full article identified by the given message ID.
    ///
    /// The `article_id` should include angle brackets (e.g. `"<abc123@example.com>"`).
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if the message ID is not found.
    pub fn article_by_id(&mut self, article_id: &str) -> Result<Article> {
        self.retrieve_article(&format!("ARTICLE {}\r\n", article_id))
    }

    /// Retrieves the full article with the given number in the currently
    /// selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists with that number.
    pub fn article_by_number(&mut self, article_number: isize) -> Result<Article> {
        self.retrieve_article(&format!("ARTICLE {}\r\n", article_number))
    }

    /// Retrieves the raw article content (headers and body as raw lines) for the
    /// given article number in the currently selected newsgroup.
    ///
    /// Unlike [`NNTPStream::article_by_number`], this returns the unparsed lines
    /// instead of a structured [`Article`].
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists with that number.
    pub fn raw_article_by_number(&mut self, article_number: isize) -> Result<Vec<String>> {
        self.retrieve_raw_article(&format!("ARTICLE {}\r\n", article_number))
    }

    /// Retrieves the body of the article indicated by the current article number
    /// in the currently selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists at the current number.
    pub fn body(&mut self) -> Result<Vec<String>> {
        self.retrieve_body("BODY\r\n")
    }

    /// Retrieves the body of the article identified by the given message ID.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if the message ID is not found.
    pub fn body_by_id(&mut self, article_id: &str) -> Result<Vec<String>> {
        self.retrieve_body(&format!("BODY {}\r\n", article_id))
    }

    /// Retrieves the body of the article with the given number in the currently
    /// selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists with that number.
    pub fn body_by_number(&mut self, article_number: isize) -> Result<Vec<String>> {
        self.retrieve_body(&format!("BODY {}\r\n", article_number))
    }

    /// Retrieves the headers of the article indicated by the current article number
    /// in the currently selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists at the current number.
    pub fn head(&mut self) -> Result<Vec<String>> {
        self.retrieve_head("HEAD\r\n")
    }

    /// Retrieves the headers of the article identified by the given message ID.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if the message ID is not found.
    pub fn head_by_id(&mut self, article_id: &str) -> Result<Vec<String>> {
        self.retrieve_head(&format!("HEAD {}\r\n", article_id))
    }

    /// Retrieves the headers of the article with the given number in the currently
    /// selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists with that number.
    pub fn head_by_number(&mut self, article_number: isize) -> Result<Vec<String>> {
        self.retrieve_head(&format!("HEAD {}\r\n", article_number))
    }

    /// Retrieves metadata (article number and message ID) for the article
    /// indicated by the current article number.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists at the current number.
    pub fn stat(&mut self) -> Result<String> {
        self.retrieve_stat("STAT\r\n")
    }

    /// Retrieves metadata for the article identified by the given message ID.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if the message ID is not found.
    pub fn stat_by_id(&mut self, article_id: &str) -> Result<String> {
        self.retrieve_stat(&format!("STAT {}\r\n", article_id))
    }

    /// Retrieves metadata for the article with the given number in the currently
    /// selected newsgroup.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ArticleUnavailable`] if no article exists with that number.
    pub fn stat_by_number(&mut self, article_number: isize) -> Result<String> {
        self.retrieve_stat(&format!("STAT {}\r\n", article_number))
    }
}

/// Information and listing commands (RFC 3977 §7)
impl NNTPStream {
    /// Retrieves the list of capabilities supported by the server.
    ///
    /// Returns a list of capability labels. Each entry may include optional
    /// arguments in parentheses.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    /// let caps = client.capabilities().expect("Failed to get capabilities");
    /// for cap in &caps {
    ///     println!("{}", cap);
    /// }
    /// ```
    pub fn capabilities(&mut self) -> Result<Vec<String>> {
        self.send_command_expect_multiline_response(
            "CAPABILITIES\r\n",
            vec![ResponseCode::CapabilitiesListFollows],
        )
    }

    /// Retrieves the server's current date and time.
    ///
    /// Returns the date in `YYYYMMDDHHMMSS` format as reported by the server.
    pub fn date(&mut self) -> Result<String> {
        self.send_command_expect_response("DATE\r\n", vec![ResponseCode::ServerDateTime])
    }

    /// Advances the current article pointer to the next article in the selected
    /// newsgroup.
    ///
    /// # Errors
    ///
    /// Returns a response error with code 421 if the current article is the last
    /// in the group, or code 412 if no group is selected.
    pub fn next_article(&mut self) -> Result<String> {
        self.send_command_expect_response("NEXT\r\n", vec![ResponseCode::ArticleExistsAndSelected])
    }

    /// Moves the current article pointer to the previous article in the selected
    /// newsgroup.
    ///
    /// # Errors
    ///
    /// Returns a response error with code 422 if the current article is the
    /// first in the group, or code 412 if no group is selected.
    pub fn last(&mut self) -> Result<String> {
        self.send_command_expect_response("LAST\r\n", vec![ResponseCode::ArticleExistsAndSelected])
    }

    /// Lists all newsgroups available on the server.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    /// let groups = client.list().expect("Failed to list groups");
    /// for group in &groups {
    ///     println!("{}", group);
    /// }
    /// ```
    pub fn list(&mut self) -> Result<Vec<NewsGroup>> {
        match self.stream.write_all(b"LIST\r\n") {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::InformationFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        match self.read_multiline_response() {
            Ok(lines) => {
                let lines: Vec<NewsGroup> = lines
                    .iter()
                    .map(|s| NewsGroup::from_list_response(s))
                    .collect();
                Ok(lines)
            }
            Err(e) => Err(e),
        }
    }

    /// Selects a newsgroup as the currently active group.
    ///
    /// After selecting a group, article retrieval commands (`article`, `body`,
    /// `head`, `stat`) operate relative to this group's article numbers.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    /// let group = client.group("comp.test")
    ///     .expect("Failed to select group");
    /// println!("{} articles in {}", group.number, group.name);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a response error with code 411 if the newsgroup does not exist.
    pub fn group(&mut self, group: &str) -> Result<NewsGroup> {
        let group_command = format!("GROUP {}\r\n", group);

        match self.stream.write_all(group_command.as_bytes()) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleNumbersFollows]) {
            Ok((_, res)) => Ok(NewsGroup::from_group_response(&res)),
            Err(e) => Err(e),
        }
    }

    /// Retrieves the server's help text.
    ///
    /// Returns a multi-line help string describing available commands.
    pub fn help(&mut self) -> Result<Vec<String>> {
        self.send_command_expect_multiline_response("HELP\r\n", vec![ResponseCode::HelpTextFollows])
    }

    /// Retrieves a list of newsgroups created since the given date and time.
    ///
    /// The `date` should be in `YYMMDD` format and `time` in `HHMMSS` format.
    /// Set `use_gmt` to `true` to interpret the time as GMT.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ResponseCode`] if the server does not support the
    /// NEWGROUPS command or the date format is invalid.
    pub fn newgroups(&mut self, date: &str, time: &str, use_gmt: bool) -> Result<Vec<String>> {
        let newgroups_command = match use_gmt {
            true => format!("NEWGROUPS {} {} GMT\r\n", date, time),
            false => format!("NEWGROUPS {} {}\r\n", date, time),
        };

        self.send_command_expect_multiline_response(
            &newgroups_command,
            vec![ResponseCode::ListOfNewNewsgroupsFollows],
        )
    }

    /// Retrieves a list of new articles posted since the given date and time
    /// in the newsgroups matching the `wildmat` pattern.
    ///
    /// The `wildmat` is a wildcard pattern (e.g. `"comp.*"`). The `date` should
    /// be in `YYMMDD` format and `time` in `HHMMSS` format.
    /// Set `use_gmt` to `true` to interpret the time as GMT.
    pub fn newnews(
        &mut self,
        wildmat: &str,
        date: &str,
        time: &str,
        use_gmt: bool,
    ) -> Result<Vec<String>> {
        let newnews_command = match use_gmt {
            true => format!("NEWNEWS {} {} {} GMT\r\n", wildmat, date, time),
            false => format!("NEWNEWS {} {} {}\r\n", wildmat, date, time),
        };

        self.send_command_expect_multiline_response(
            &newnews_command,
            vec![ResponseCode::ListOfNewArticlesFollows],
        )
    }

    /// Closes the connection to the NNTP server.
    ///
    /// Sends the `QUIT` command and waits for the server's closing response.
    /// After calling this method, the stream should not be used for further
    /// commands.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    /// // ... do work ...
    /// let _ = client.quit();
    /// ```
    pub fn quit(&mut self) -> Result<()> {
        match self.stream.write_all(b"QUIT\r\n") {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ConnectionClosing]) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Sends the `MODE` command to the server.
    ///
    /// The `mode` argument should be `"READER"` or `"POSTER"` (case-insensitive).
    /// `MODE READER` tells the server that the client is a news reader, while
    /// `MODE POSTER` indicates the client intends to post articles.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ResponseCode`] if the server does not support the
    /// requested mode.
    pub fn set_mode(&mut self, mode: &str) -> Result<String> {
        let mode_upper = mode.to_uppercase();
        self.send_command_expect_response(
            &format!("MODE {}\r\n", mode_upper),
            vec![
                ResponseCode::ServiceAvailablePostingAllowed,
                ResponseCode::ServiceAvailablePostingProhibited,
            ],
        )
    }

    /// Sends `MODE READER` to indicate the client is a news reader.
    ///
    /// This is typically issued before authentication. The server responds
    /// with code 200 (posting allowed) or 201 (posting prohibited).
    pub fn set_mode_reader(&mut self) -> Result<String> {
        self.send_command_expect_response(
            "MODE READER\r\n",
            vec![
                ResponseCode::ServiceAvailablePostingAllowed,
                ResponseCode::ServiceAvailablePostingProhibited,
            ],
        )
    }

    /// Sends `MODE POSTER` to indicate the client intends to post articles.
    ///
    /// The server will respond with code 200 if posting is allowed.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::ResponseCode`] (502) if posting is not permitted.
    pub fn set_mode_poster(&mut self) -> Result<String> {
        self.send_command_expect_response(
            "MODE POSTER\r\n",
            vec![ResponseCode::ServiceAvailablePostingAllowed],
        )
    }
}

/// Authentication commands (RFC 4643)
impl NNTPStream {
    /// Sends `AUTHINFO USER` to the server.
    ///
    /// Accepts either [`ResponseCode::AuthenticationAccepted`] (281, server
    /// accepts without password) or [`ResponseCode::PasswordRequired`] (381,
    /// password needed).
    fn auth_user(&mut self, username: &str) -> Result<String> {
        self.send_command_expect_response(
            &format!("AUTHINFO USER {}\r\n", username),
            vec![
                ResponseCode::AuthenticationAccepted,
                ResponseCode::PasswordRequired,
            ],
        )
    }

    /// Sends `AUTHINFO PASS` to the server.
    ///
    /// Expects [`ResponseCode::AuthenticationAccepted`] (281) on success.
    fn auth_password(&mut self, password: &str) -> Result<String> {
        self.send_command_expect_response(
            &format!("AUTHINFO PASS {}\r\n", password),
            vec![ResponseCode::AuthenticationAccepted],
        )
    }

    // TODO: implement SASL ?
}

/// Posting commands (RFC 3977 §5)
impl NNTPStream {
    /// Posts a message to the currently selected newsgroup.
    ///
    /// The `message` must be a complete article including headers and body,
    /// terminated with `\r\n.\r\n` (a line containing only a dot).
    ///
    /// # Message format
    ///
    /// The message should include standard headers such as `From`, `Newsgroups`,
    /// `Subject`, and `Date`. The server will validate these before accepting
    /// the post.
    ///
    /// # Errors
    ///
    /// Returns [`NNTPError::InvalidMessage`] if the message does not end with
    /// the required `\r\n.\r\n` terminator.
    /// Returns a response error with code 440 if the server does not allow posting.
    /// Returns a response error with code 441 if the server rejects the message content.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nntp::NNTPStream;
    ///
    /// let mut client = NNTPStream::connect("nntp.example.com:119".to_string())
    ///     .expect("Failed to connect");
    ///
    /// let message = "From: user@example.com\r\n\
    ///                  Newsgroups: comp.test\r\n\
    ///                  Subject: Test post\r\n\
    ///                  \r\n\
    ///                  This is a test.\r\n\
    ///                  .\r\n";
    /// client.post(message).expect("Failed to post");
    /// ```
    pub fn post(&mut self, message: &str) -> Result<()> {
        if !self.is_valid_message(message) {
            return Err(NNTPError::InvalidMessage {
                message: message.to_owned(),
                reason: "Invalid message format. Message must end with \"\r\n.\r\n\"".to_owned(),
            });
        }

        match self.stream.write_all(b"POST\r\n") {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::SendArticleToPost]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        match self.stream.write_all(message.as_bytes()) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleReceivedOK]) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// base protocol handling helpers
impl NNTPStream {
    fn send_command_expect_response(
        &mut self,
        command: &str,
        expected_code: Vec<codes::ResponseCode>,
    ) -> Result<String> {
        match self.stream.write_all(command.as_bytes()) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(expected_code) {
            Ok((_, message)) => Ok(message),
            Err(e) => Err(e),
        }
    }

    fn send_command_expect_multiline_response(
        &mut self,
        command: &str,
        expected_code: Vec<codes::ResponseCode>,
    ) -> Result<Vec<String>> {
        match self.stream.write_all(command.as_bytes()) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(expected_code) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    fn retrieve_article(&mut self, article_command: &str) -> Result<Article> {
        match self.stream.write_all(article_command.as_bytes()) {
            Ok(_) => (),
            Err(error) => return Err(errors::article_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleFollows]) {
            Ok(_) => (),
            Err(e) => match e {
                // TODO: replace by status code evaluation
                NNTPError::ResponseCode {
                    expected: _,
                    received: 423,
                } => return Err(errors::NNTPError::ArticleUnavailable),
                _ => return Err(e),
            },
        }

        match self.read_multiline_response() {
            Ok(lines) => Ok(Article::new_article(lines)),
            Err(e) => Err(e),
        }
    }

    fn retrieve_raw_article(&mut self, article_command: &str) -> Result<Vec<String>> {
        match self.stream.write_all(article_command.as_bytes()) {
            Ok(_) => (),
            Err(error) => return Err(errors::article_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleFollows]) {
            Ok(_) => (),
            Err(e) => match e {
                // TODO: replace by status code evaluation
                NNTPError::ResponseCode {
                    expected: _,
                    received: 423,
                } => return Err(errors::NNTPError::ArticleUnavailable),
                _ => return Err(e),
            },
        }

        match self.read_multiline_response() {
            Ok(lines) => Ok(lines),
            Err(e) => Err(e),
        }
    }

    fn retrieve_body(&mut self, body_command: &str) -> Result<Vec<String>> {
        self.send_command_expect_multiline_response(
            body_command,
            vec![ResponseCode::ArticleBodyFollows],
        )
    }

    fn retrieve_head(&mut self, head_command: &str) -> Result<Vec<String>> {
        self.send_command_expect_multiline_response(
            head_command,
            vec![ResponseCode::ArticleHeadersFollows],
        )
    }

    fn retrieve_stat(&mut self, stat_command: &str) -> Result<String> {
        self.send_command_expect_response(
            stat_command,
            vec![ResponseCode::ArticleExistsAndSelected],
        )
    }

    fn is_valid_message(&self, message: &str) -> bool {
        //Carriage return
        let cr = 0x0d;
        //Line Feed
        let lf = 0x0a;
        //Dot
        let dot = 0x2e;
        let message_bytes = message.as_bytes();
        let length = message_bytes.len();

        length >= 5
            && (message_bytes[length - 1] == lf
                && message_bytes[length - 2] == cr
                && message_bytes[length - 3] == dot
                && message_bytes[length - 4] == lf
                && message_bytes[length - 5] == cr)
    }

    // Retrieve single line response
    // response matching any of the expected_code will be considered valid
    fn read_response(
        &mut self,
        expected_code: Vec<codes::ResponseCode>,
    ) -> Result<(isize, String)> {
        //Carriage return
        let cr = 0x0d;
        //Line Feed
        let lf = 0x0a;
        let mut line_buffer: Vec<u8> = Vec::new();

        while line_buffer.len() < 2
            || (line_buffer[line_buffer.len() - 1] != lf
                && line_buffer[line_buffer.len() - 2] != cr)
        {
            let byte_buffer: &mut [u8] = &mut [0];
            match self.stream.read(byte_buffer) {
                Ok(_) => {}
                Err(error) => return Err(errors::response_error_or_network(error)),
            }
            line_buffer.push(byte_buffer[0]);
        }

        // Try to detect encoding and convert to UTF-8
        // First try UTF-8, then fall back to WINDOWS-1252 (common in Usenet)
        let (mut decoded_text, _, mut had_errors) = encoding_rs::UTF_8.decode(&line_buffer);

        if had_errors {
            // UTF-8 failed, try WINDOWS-1252
            (decoded_text, _, had_errors) = encoding_rs::WINDOWS_1252.decode(&line_buffer);

            if had_errors {
                // error again ?
                return Err(NNTPError::DecodingError);
            }
        }
        let response = decoded_text.to_string();
        let chars_to_trim: &[char] = &['\r', '\n'];
        let trimmed_response = response.trim_matches(chars_to_trim);
        let trimmed_response_vec: Vec<char> = trimmed_response.chars().collect();
        if trimmed_response_vec.len() < 5 || trimmed_response_vec[3] != ' ' {
            return Err(NNTPError::InvalidResponse {
                response: trimmed_response_vec.into_iter().collect(),
            });
        }

        let response_parts: Vec<&str> = trimmed_response.splitn(2, ' ').collect();

        let code = response_parts[0].parse::<isize>();
        match code {
            Ok(code) => {
                let message = response_parts[1];
                if expected_code.iter().any(|&exp| exp as isize == code) {
                    Ok((code, message.to_string()))
                } else {
                    Err(NNTPError::ResponseCode {
                        expected: expected_code,
                        received: code,
                    })
                }
            }
            Err(e) => {
                log::warn!(
                    "error parsing '{}' as a ResponseCode: {e}",
                    response_parts[0]
                );
                Err(NNTPError::InvalidResponse {
                    response: trimmed_response.to_string(),
                })
            }
        }
    }

    fn read_multiline_response(&mut self) -> Result<Vec<String>> {
        let mut response: Vec<String> = Vec::new();
        //Carriage return
        let cr = 0x0d;
        //Line Feed
        let lf = 0x0a;
        let mut line_buffer: Vec<u8> = Vec::new();
        let mut complete = false;

        while !complete {
            while line_buffer.len() < 2
                || (line_buffer[line_buffer.len() - 1] != lf
                    && line_buffer[line_buffer.len() - 2] != cr)
            {
                let byte_buffer: &mut [u8] = &mut [0];
                match self.stream.read(byte_buffer) {
                    Ok(_) => {}
                    Err(error) => return Err(errors::response_error_or_network(error)),
                }
                line_buffer.push(byte_buffer[0]);
            }

            // Try to detect encoding and convert to UTF-8
            // First try UTF-8, then fall back to WINDOWS-1252 (common in Usenet)
            let (mut decoded_text, _, mut had_errors) = encoding_rs::UTF_8.decode(&line_buffer);

            if had_errors {
                // UTF-8 failed, try WINDOWS-1252
                (decoded_text, _, had_errors) = encoding_rs::WINDOWS_1252.decode(&line_buffer);

                if had_errors {
                    // error again ?
                    return Err(NNTPError::DecodingError);
                }
            }
            let decoded_text = decoded_text.to_string();
            if decoded_text == ".\r\n" {
                complete = true;
            } else {
                response.push(decoded_text);
                line_buffer = Vec::new();
            }
        }
        Ok(response)
    }
}
