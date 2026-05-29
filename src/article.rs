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
                if let Some(ref key) = last_key
                    && (trimmed.starts_with(' ') || trimmed.starts_with('\t'))
                {
                    if let Some(existing) = headers.get_mut(key) {
                        existing.push('\n');
                        existing.push_str(trimmed);
                    }
                    continue;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_headers_and_body() {
        let raw = vec![
            "From: user@example.com\r\n".to_string(),
            "Subject: Hello\r\n".to_string(),
            "\r\n".to_string(),
            "Hello world!\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        assert_eq!(
            article.headers.get("From"),
            Some(&" user@example.com".to_string())
        );
        assert_eq!(article.headers.get("Subject"), Some(&" Hello".to_string()));
        assert_eq!(article.body, vec!["Hello world!\r\n".to_string()]);
    }

    #[test]
    fn test_header_with_colon_in_value() {
        let raw = vec![
            "Message-ID: <abc:def@example.com>\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        assert_eq!(
            article.headers.get("Message-ID"),
            Some(&" <abc:def@example.com>".to_string())
        );
    }

    #[test]
    fn test_header_continuation_with_space() {
        let raw = vec![
            "Subject: This is a very long\r\n".to_string(),
            " subject line\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        let subject = article.headers.get("Subject").unwrap();
        assert!(subject.contains("This is a very long"));
        assert!(subject.contains("subject line"));
        assert!(subject.contains('\n'));
    }

    #[test]
    fn test_header_continuation_with_tab() {
        let raw = vec![
            "References: <msg1@ex.com>\r\n".to_string(),
            "\t<msg2@ex.com>\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        let refs = article.headers.get("References").unwrap();
        assert!(refs.contains("<msg1@ex.com>"));
        assert!(refs.contains("<msg2@ex.com>"));
    }

    #[test]
    fn test_multiple_continuations() {
        let raw = vec![
            "X-Long: first part\r\n".to_string(),
            " second part\r\n".to_string(),
            " third part\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        let val = article.headers.get("X-Long").unwrap();
        assert!(val.contains("first part"));
        assert!(val.contains("second part"));
        assert!(val.contains("third part"));
        assert_eq!(val.matches('\n').count(), 2);
    }

    #[test]
    fn test_header_without_colon_skipped() {
        let raw = vec![
            "From: sender@example.com\r\n".to_string(),
            "This line has no colon\r\n".to_string(),
            "Subject: test\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        assert_eq!(
            article.headers.get("From"),
            Some(&" sender@example.com".to_string())
        );
        assert_eq!(article.headers.get("Subject"), Some(&" test".to_string()));
        assert!(!article.headers.contains_key("This"));
    }

    #[test]
    fn test_empty_header_line_skipped() {
        let raw = vec![
            "From: sender@example.com\r\n".to_string(),
            "\r\n".to_string(),
            "Subject: test\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        assert_eq!(article.headers.len(), 1);
        assert_eq!(
            article.headers.get("From"),
            Some(&" sender@example.com".to_string())
        );
        assert!(!article.headers.contains_key("Subject"));
        assert_eq!(
            article.body,
            vec!["Subject: test\r\n".to_string(), "body\r\n".to_string()]
        );
    }

    #[test]
    fn test_lines_with_crlf_terminators() {
        let raw = vec![
            "From: user@example.com\r\n".to_string(),
            "Subject: Hello\r\n".to_string(),
            "\r\n".to_string(),
            "Line 1\r\n".to_string(),
            "Line 2\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        assert_eq!(article.body.len(), 2);
        assert_eq!(article.body[0], "Line 1\r\n");
        assert_eq!(article.body[1], "Line 2\r\n");
    }

    #[test]
    fn test_empty_body() {
        let raw = vec!["From: user@example.com\r\n".to_string(), "\r\n".to_string()];
        let article = Article::new_article(raw);
        assert!(article.body.is_empty());
    }

    #[test]
    fn test_no_headers() {
        let raw = vec!["\r\n".to_string(), "body\r\n".to_string()];
        let article = Article::new_article(raw);
        assert!(article.headers.is_empty());
        assert_eq!(article.body, vec!["body\r\n".to_string()]);
    }

    #[test]
    fn test_continuation_after_header_with_trimmed_crlf() {
        let raw = vec![
            "X-Folded: start\r\n".to_string(),
            " continue\r\n".to_string(),
            " more\r\n".to_string(),
            "\r\n".to_string(),
            "body\r\n".to_string(),
        ];
        let article = Article::new_article(raw);
        let val = article.headers.get("X-Folded").unwrap();
        assert!(val.starts_with(" start"));
        assert!(val.contains("\n continue"));
        assert!(val.contains("\n more"));
        assert!(val.contains(" continue"));
        assert!(val.contains(" more"));
    }
}
