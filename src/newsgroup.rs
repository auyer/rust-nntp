use std::fmt;
use std::str::FromStr;
use std::string::String;

/// Information about a Usenet newsgroup.
///
/// A newsgroup is a discussion forum on a specific topic. Each group has a
/// hierarchical name (e.g. `comp.lang.rust`) and tracks article numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsGroup {
    /// The name of the newsgroup (e.g. `"comp.lang.rust"`).
    pub name: String,
    /// The highest article number in the group.
    pub high: isize,
    /// The lowest article number in the group.
    pub low: isize,
    /// The estimated number of articles in the group (`high - low`).
    pub number: isize,
    /// The posting status of the group (e.g. `"y"` for yes, `"m"` for moderated).
    /// Empty when parsed from `GROUP` command responses.
    pub status: String,
}

impl fmt::Display for NewsGroup {
    /// Formats the newsgroup as `"name (article_count)"`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.high - self.low)
    }
}

impl NewsGroup {
    /// Parses a newsgroup from a `LIST` command response line.
    ///
    /// The expected format is: `group high low status`
    ///
    /// # Panics
    ///
    /// Panics if the `high` or `low` fields cannot be parsed as integers.
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

    /// Parses a newsgroup from a `GROUP` command response.
    ///
    /// The expected format is: `211 number low high group`
    /// (the response code prefix is stripped before parsing).
    ///
    /// # Panics
    ///
    /// Panics if the numeric fields cannot be parsed as integers.
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
