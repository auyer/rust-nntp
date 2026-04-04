use std::io::{Read, Write};
use std::net::TcpStream;
use std::string::String;
use std::vec::Vec;

use crate::article::Article;
use crate::codes::{self, ResponseCode};
use crate::connection::connect_with_retry;
use crate::errors::{self, NNTPError, Result};
use crate::newsgroup::NewsGroup;

/// Stream to be used for interfacing with a NNTP server.
pub struct NNTPStream {
    server_address: String,
    stream: TcpStream,
}

impl NNTPStream {
    /// Creates an NNTP Stream.
    pub fn connect(addr: String) -> Result<NNTPStream> {
        let tcp_stream = connect_with_retry(&addr, 3, 7_0000, 100)?;
        let mut socket = NNTPStream {
            stream: tcp_stream,
            server_address: addr,
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

    pub fn re_connect(&mut self) -> Result<()> {
        let tcp_stream = connect_with_retry(&self.server_address, 3, 7_000, 100)?;
        self.stream = tcp_stream;

        match self.read_response(vec![
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
        }
    }

    /// The article indicated by the current article number in the currently selected newsgroup is selected.
    pub fn article(&mut self) -> Result<Article> {
        self.retrieve_article("ARTICLE\r\n")
    }

    /// The article indicated by the article id is selected.
    pub fn article_by_id(&mut self, article_id: &str) -> Result<Article> {
        self.retrieve_article(&format!("ARTICLE {}\r\n", article_id))
    }

    /// The article indicated by the article number in the currently selected newsgroup is selected.
    pub fn article_by_number(&mut self, article_number: isize) -> Result<Article> {
        self.retrieve_article(&format!("ARTICLE {}\r\n", article_number))
    }

    /// The article indicated by the article number in the currently selected newsgroup is selected.
    /// returns the raw email line by line
    pub fn raw_article_by_number(&mut self, article_number: isize) -> Result<Vec<String>> {
        self.retrieve_raw_article(&format!("ARTICLE {}\r\n", article_number))
    }

    fn retrieve_article(&mut self, article_command: &str) -> Result<Article> {
        match self.stream.write_fmt(format_args!("{}", article_command)) {
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
        match self.stream.write_fmt(format_args!("{}", article_command)) {
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

    /// Retrieves the body of the current article number in the currently selected newsgroup.
    pub fn body(&mut self) -> Result<Vec<String>> {
        self.retrieve_body("BODY\r\n")
    }

    /// Retrieves the body of the article id.
    pub fn body_by_id(&mut self, article_id: &str) -> Result<Vec<String>> {
        self.retrieve_body(&format!("BODY {}\r\n", article_id))
    }

    /// Retrieves the body of the article number in the currently selected newsgroup.
    pub fn body_by_number(&mut self, article_number: isize) -> Result<Vec<String>> {
        self.retrieve_body(&format!("BODY {}\r\n", article_number))
    }

    fn retrieve_body(&mut self, body_command: &str) -> Result<Vec<String>> {
        match self.stream.write_fmt(format_args!("{}", body_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleBodyFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    /// Gives the list of capabilities that the server has.
    pub fn capabilities(&mut self) -> Result<Vec<String>> {
        let capabilities_command = "CAPABILITIES\r\n".to_string();

        match self
            .stream
            .write_fmt(format_args!("{}", capabilities_command))
        {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::CapabilitiesListFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    /// Retrieves the date as the server sees the date.
    pub fn date(&mut self) -> Result<String> {
        let date_command = "DATE\r\n".to_string();

        match self.stream.write_fmt(format_args!("{}", date_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ServerDateTime]) {
            Ok((_, message)) => Ok(message),
            Err(e) => Err(e),
        }
    }

    /// Retrieves the headers of the current article number in the currently selected newsgroup.
    pub fn head(&mut self) -> Result<Vec<String>> {
        self.retrieve_head("HEAD\r\n")
    }

    /// Retrieves the headers of the article id.
    pub fn head_by_id(&mut self, article_id: &str) -> Result<Vec<String>> {
        self.retrieve_head(&format!("HEAD {}\r\n", article_id))
    }

    /// Retrieves the headers of the article number in the currently selected newsgroup.
    pub fn head_by_number(&mut self, article_number: isize) -> Result<Vec<String>> {
        self.retrieve_head(&format!("HEAD {}\r\n", article_number))
    }

    fn retrieve_head(&mut self, head_command: &str) -> Result<Vec<String>> {
        match self.stream.write_fmt(format_args!("{}", head_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleHeadersFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    /// Moves the currently selected article number back one
    pub fn last(&mut self) -> Result<String> {
        let last_command = "LAST\r\n".to_string();

        match self.stream.write_fmt(format_args!("{}", last_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleExistsAndSelected]) {
            Ok((_, message)) => Ok(message),
            Err(e) => Err(e),
        }
    }

    /// Lists all of the newgroups on the server.
    pub fn list(&mut self) -> Result<Vec<NewsGroup>> {
        let list_command = "LIST\r\n".to_string();

        match self.stream.write_fmt(format_args!("{}", list_command)) {
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
                    .map(|ref mut x| NewsGroup::from_list_response(x))
                    .collect();
                Ok(lines)
            }
            Err(e) => Err(e),
        }
    }

    /// Selects a newsgroup
    pub fn group(&mut self, group: &str) -> Result<NewsGroup> {
        let group_command = format!("GROUP {}\r\n", group);

        match self.stream.write_fmt(format_args!("{}", group_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        };

        match self.read_response(vec![ResponseCode::ArticleNumbersFollows]) {
            Ok((_, res)) => Ok(NewsGroup::from_group_response(&res)),
            Err(e) => Err(e),
        }
    }

    /// Show the help command given on the server.
    pub fn help(&mut self) -> Result<Vec<String>> {
        let help_command = "HELP\r\n".to_string();

        match self.stream.write_fmt(format_args!("{}", help_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::HelpTextFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    /// Quits the current session.
    pub fn quit(&mut self) -> Result<()> {
        let quit_command = "QUIT\r\n".to_string();
        match self.stream.write_fmt(format_args!("{}", quit_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ConnectionClosing]) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Retrieves a list of newsgroups since the date and time given.
    pub fn newgroups(&mut self, date: &str, time: &str, use_gmt: bool) -> Result<Vec<String>> {
        let newgroups_command = match use_gmt {
            true => format!("NEWSGROUP {} {} GMT\r\n", date, time),
            false => format!("NEWSGROUP {} {}\r\n", date, time),
        };

        match self.stream.write_fmt(format_args!("{}", newgroups_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ListOfNewNewsgroupsFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    /// Retrieves a list of new news since the date and time given.
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

        match self.stream.write_fmt(format_args!("{}", newnews_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ListOfNewArticlesFollows]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        self.read_multiline_response()
    }

    /// Moves the currently selected article number forward one
    pub fn next_article(&mut self) -> Result<String> {
        let next_command = "NEXT\r\n".to_string();
        match self.stream.write_fmt(format_args!("{}", next_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleExistsAndSelected]) {
            Ok((_, message)) => Ok(message),
            Err(e) => Err(e),
        }
    }

    /// Posts a message to the NNTP server.
    pub fn post(&mut self, message: &str) -> Result<()> {
        if !self.is_valid_message(message) {
            return Err(NNTPError::InvalidMessage {
                message: message.to_owned(),
                reason: "Invalid message format. Message must end with \"\r\n.\r\n\"".to_owned(),
            });
        }

        let post_command = "POST\r\n".to_string();

        match self.stream.write_fmt(format_args!("{}", post_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::SendArticleToPost]) {
            Ok(_) => (),
            Err(e) => return Err(e),
        };

        match self.stream.write_fmt(format_args!("{}", message)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleReceivedOK]) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Gets information about the current article.
    pub fn stat(&mut self) -> Result<String> {
        self.retrieve_stat("STAT\r\n")
    }

    /// Gets the information about the article id.
    pub fn stat_by_id(&mut self, article_id: &str) -> Result<String> {
        self.retrieve_stat(&format!("STAT {}\r\n", article_id))
    }

    /// Gets the information about the article number.
    pub fn stat_by_number(&mut self, article_number: isize) -> Result<String> {
        self.retrieve_stat(&format!("STAT {}\r\n", article_number))
    }

    fn retrieve_stat(&mut self, stat_command: &str) -> Result<String> {
        match self.stream.write_fmt(format_args!("{}", stat_command)) {
            Ok(_) => (),
            Err(error) => return Err(errors::write_error_or_network(error)),
        }

        match self.read_response(vec![ResponseCode::ArticleExistsAndSelected]) {
            Ok((_, message)) => Ok(message),
            Err(e) => Err(e),
        }
    }

    fn is_valid_message(&mut self, message: &str) -> bool {
        //Carriage return
        let cr = 0x0d;
        //Line Feed
        let lf = 0x0a;
        //Dot
        let dot = 0x2e;
        let message_string = message.to_string();
        let message_bytes = message_string.as_bytes();
        let length = message_string.len();

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
