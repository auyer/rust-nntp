use std::collections::HashMap;
use std::string::String;
use std::vec::Vec;

/// A parsed NNTP article.
///
/// Articles consist of a set of headers (key-value pairs) followed by a body.
/// The [`Article::new_article`] constructor parses raw response lines into this
/// structured form, splitting on the first blank line (`\r\n`) between headers
/// and body.
///
/// # Example
///
/// ```
/// use nntp::Article;
///
/// let raw = vec![
///     "From: user@example.com\r\n".to_string(),
///     "Subject: Hello\r\n".to_string(),
///     "\r\n".to_string(),
///     "Hello world!\r\n".to_string(),
/// ];
/// let article = Article::new_article(raw);
/// assert_eq!(article.headers.get("From"), Some(&" user@example.com".to_string()));
/// assert_eq!(article.headers.get("Subject"), Some(&" Hello".to_string()));
/// assert_eq!(article.body, vec!["Hello world!\r\n".to_string()]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// Article headers as a map of header name to header value.
    pub headers: HashMap<String, String>,
    /// Article body lines (including trailing `\r\n` on each line).
    pub body: Vec<String>,
}

impl Article {
    /// Parses raw article lines into an [`Article`].
    ///
    /// Lines before the first blank line (`\r\n`) are treated as headers.
    /// Each header line is split on the first `:` to separate the key and value.
    /// Lines after the blank line are treated as body content.
    ///
    /// # Arguments
    ///
    /// * `lines` - Raw article lines as returned by the server, including `\r\n` terminators.
    pub fn new_article(lines: Vec<String>) -> Article {
        let mut headers: HashMap<String, String> = HashMap::new();
        let mut body = Vec::new();
        let mut parsing_headers = true;
        let mut last_key: Option<String> = None;
        let chars_to_trim: &[char] = &['\r', '\n'];

        for i in lines.iter() {
            if i == &"\r\n".to_string() {
                parsing_headers = false;
                continue;
            }
            if parsing_headers {
                let trimmed = i.trim_matches(chars_to_trim);
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(ref key) = last_key {
                    if trimmed.starts_with(' ') || trimmed.starts_with('\t') {
                        if let Some(existing) = headers.get_mut(key) {
                            existing.push('\n');
                            existing.push_str(trimmed);
                        }
                        continue;
                    }
                }
                if let Some((key, value)) = i.split_once(':') {
                    let key = key.trim_matches(chars_to_trim).to_string();
                    let value = value.trim_matches(chars_to_trim).to_string();
                    last_key = Some(key.clone());
                    headers.insert(key, value);
                }
            } else {
                body.push(i.clone());
            }
        }
        Article { headers, body }
    }
}
