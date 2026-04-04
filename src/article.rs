use std::collections::HashMap;
use std::string::String;
use std::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    pub headers: HashMap<String, String>,
    pub body: Vec<String>,
}

impl Article {
    pub fn new_article(lines: Vec<String>) -> Article {
        let mut headers = HashMap::new();
        let mut body = Vec::new();
        let mut parsing_headers = true;

        for i in lines.iter() {
            if i == &"\r\n".to_string() {
                parsing_headers = false;
                continue;
            }
            if parsing_headers {
                let mut header = i.splitn(2, ':');
                let chars_to_trim: &[char] = &['\r', '\n'];
                let key = header
                    .nth(0)
                    .unwrap()
                    .trim_matches(chars_to_trim)
                    .to_string();
                let value = header
                    .nth(0)
                    .unwrap()
                    .trim_matches(chars_to_trim)
                    .to_string();
                headers.insert(key, value);
            } else {
                body.push(i.clone());
            }
        }
        Article { headers, body }
    }
}
