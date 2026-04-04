use std::fmt;
use std::str::FromStr;
use std::string::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsGroup {
    pub name: String,
    pub high: isize,
    pub low: isize,
    pub number: isize,
    pub status: String,
}

impl fmt::Display for NewsGroup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.high - self.low)
    }
}

impl NewsGroup {
    pub fn from_list_response(group: &str) -> NewsGroup {
        // group high low status
        let chars_to_trim: &[char] = &['\r', '\n', ' '];
        let trimmed_group = group.trim_matches(chars_to_trim);
        let split_group: Vec<&str> = trimmed_group.split(' ').collect();

        let high: isize = FromStr::from_str(split_group[1]).unwrap();
        let low: isize = FromStr::from_str(split_group[2]).unwrap();
        NewsGroup {
            name: split_group[0].to_string(),
            high,
            low,
            number: high - low,
            status: split_group[3].to_string(),
        }
    }

    pub fn from_group_response(group: &str) -> NewsGroup {
        // 211 number low high group
        let chars_to_trim: &[char] = &['\r', '\n', ' '];
        let trimmed_group = group.trim_matches(chars_to_trim);
        let split_group: Vec<&str> = trimmed_group.split(' ').collect();
        NewsGroup {
            number: FromStr::from_str(split_group[0]).unwrap(),
            low: FromStr::from_str(split_group[1]).unwrap(),
            high: FromStr::from_str(split_group[2]).unwrap(),
            name: split_group[3].to_string(),
            // status not returned in this command
            status: "".to_owned(),
        }
    }
}
