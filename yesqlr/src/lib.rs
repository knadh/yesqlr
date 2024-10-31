//! # yesqlr
//!
//! yesqlr is a Rust port of the [goyesql](https://github.com/knadh/goyesql) Go library.
//! It allows multiple SQL queries to be defined in an `.sql` file, each separate by a specially formatted `--name: $name`
//! accompanying every query, which the library then parses to a HashMap<$name, Query{}>.
//! In addition, it also supports attaching arbitrary --$key: $value tags with every query
//! This allows better organization and handling of SQL code in Rust projects.
//!
//!
//! ## Usage
//!
//! Create a `.sql` file with multiple queries, each preceded by a `-- name: query_name` tag. Additional tags can be added as needed.
//!
//! ```sql
//! -- name: get_user
//! -- raw: true
//! SELECT * FROM users WHERE id = $1;
//!
//! -- name: create_user
//! INSERT INTO users (name, email) VALUES ($1, $2);
//! ```
//!
//! ### Parsing SQL files
//!
//! Use the `parse_file()` function to read and parse the `.sql` file.
//!
//! ```rust
//! use yesqlr::parse_file;
//!
//! fn main() -> Result<(), yesqlr::ParseError> {
//!     let queries = parse_file("test.sql").expect("error parsing file");
//!     let q = &queries["simple"].query;
//!     println!("the query is: {}", q);
//!     Ok(())
//! }
//! ```
//!
//! ### Parsing bytes / Reader
//!
//! Alternatively, parse SQL queries from a byte stream using the `parse()` function.
//!
//! ```rust
//! use yesqlr::parse;
//!
//! fn main() -> Result<(), yesqlr::ParseError> {
//!     let raq = b"-- name: list_users\nSELECT * FROM users;";
//!     let queries = parse(&raq[..])?;
//!     let list_users_query = &queries["list_users"].query;
//!     println!("user query is: {}", list_users_query);
//!     Ok(())
//! }
//! ```
//!
//! ## License
//!
//! This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

const TAG_NAME: &str = "name";

lazy_static! {
    static ref RE_TAG: Regex = Regex::new(r"^\s*--\s*(.+)\s*:\s*(.+)").unwrap();
    static ref RE_COMMENT: Regex = Regex::new(r"^\s*--\s*(.*)").unwrap();
}

// Represents an parse error.
#[derive(Debug)]
pub struct ParseError(String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// Represents a single SQL query parsed from the file with associated tags.
///
/// # Fields
///
/// - `query`: The SQL query string.
/// - `tags`: A map of tag names to their corresponding values.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub query: String,
    pub tags: HashMap<String, String>,
}

// Map of query names (--name from the file) to the Query.
pub type Queries = HashMap<String, Query>;

#[derive(Debug, PartialEq)]
enum LineType {
    Blank,
    Query,
    Comment,
    Tag,
}

#[derive(Debug)]
struct ParsedLine {
    line_type: LineType,
    tag: String,
    value: String,
}

/// Parses a `.sql` file and returns a map of queries indexed by their `--name` tag.
///
/// # Arguments
///
/// * `path` - The path to the `.sql` file to parse.
///
/// # Errors
///
/// Returns a `ParseError` if the file cannot be read or if the content is malformed.
///
/// # Examples
///
/// ```rust
/// use yesqlr::parse_file;
///
/// let queries = parse_file("test.sql").expect("error parsing file");
/// ```
pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Queries, ParseError> {
    let file = File::open(path).map_err(|e| ParseError(format!("error reading file: {}", e)))?;
    parse(file)
}

/// Parses the given bytes and returns a map of queries indexed by their `--name` tag.
///
/// # Arguments
///
/// * `path` - The path to the `.sql` file to parse.
///
/// # Errors
///
/// Returns a `ParseError` if the file cannot be read or if the content is malformed.
///
/// # Examples
///
/// ```rust
/// use yesqlr::parse;
///
/// let queries = parse("--name: test\nSELECT 1;".as_bytes()).expect("error parsing bytes");
/// ```

pub fn parse<R: Read>(reader: R) -> Result<Queries, ParseError> {
    let mut name = String::new();
    let mut queries = Queries::new();

    for (i, line) in BufReader::new(reader).lines().enumerate() {
        let line = line.map_err(|e| ParseError(format!("error reading line {}: {}", i + 1, e)))?;
        let parsed_line = parse_line(&line);

        match parsed_line.line_type {
            LineType::Blank | LineType::Comment => continue,
            LineType::Query => {
                if name.is_empty() {
                    return Err(ParseError(format!(
                        "query is missing the 'name' tag: {}",
                        parsed_line.value
                    )));
                }
                let q = queries.entry(name.clone()).or_insert(Query {
                    query: String::new(),
                    tags: HashMap::new(),
                });
                if !q.query.is_empty() {
                    q.query.push(' ');
                }
                q.query.push_str(&parsed_line.value);
            }
            LineType::Tag => {
                if parsed_line.tag == TAG_NAME {
                    name = parsed_line.value.clone();
                    if queries.contains_key(&name) {
                        return Err(ParseError(format!(
                            "duplicate tag {} = {}",
                            parsed_line.tag, parsed_line.value
                        )));
                    }

                    queries.insert(
                        name.clone(),
                        Query {
                            query: String::new(),
                            tags: HashMap::new(),
                        },
                    );
                } else {
                    if !queries.contains_key(&name) {
                        return Err(ParseError("'name' should be the first tag".to_string()));
                    }

                    let q = queries.get_mut(&name).unwrap();
                    if q.tags.contains_key(&parsed_line.tag) {
                        return Err(ParseError(format!(
                            "duplicate tag {} = {}",
                            parsed_line.tag, parsed_line.value
                        )));
                    }
                    q.tags.insert(parsed_line.tag, parsed_line.value);
                }
            }
        }
    }

    for (name, query) in &queries {
        if query.query.is_empty() {
            return Err(ParseError(format!("'{}' is missing query", name)));
        }
    }

    Ok(queries)
}

// Parse a single line while iterating the raw SQL bytes.
fn parse_line(line: &str) -> ParsedLine {
    let line = line.trim();
    if line.is_empty() {
        return ParsedLine {
            line_type: LineType::Blank,
            tag: String::new(),
            value: String::new(),
        };
    }

    if let Some(captures) = RE_TAG.captures(line) {
        ParsedLine {
            line_type: LineType::Tag,
            tag: captures[1].to_string(),
            value: captures[2].to_string(),
        }
    } else if let Some(captures) = RE_COMMENT.captures(line) {
        ParsedLine {
            line_type: LineType::Comment,
            tag: String::new(),
            value: captures[1].to_string(),
        }
    } else {
        ParsedLine {
            line_type: LineType::Query,
            tag: String::new(),
            value: line.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line() {
        let tests = vec![
            (
                " ",
                ParsedLine {
                    line_type: LineType::Blank,
                    tag: String::new(),
                    value: String::new(),
                },
            ),
            (
                " SELECT * ",
                ParsedLine {
                    line_type: LineType::Query,
                    tag: String::new(),
                    value: "SELECT *".to_string(),
                },
            ),
            (
                " -- name: tag ",
                ParsedLine {
                    line_type: LineType::Tag,
                    tag: "name".to_string(),
                    value: "tag".to_string(),
                },
            ),
            (
                " -- some: param ",
                ParsedLine {
                    line_type: LineType::Tag,
                    tag: "some".to_string(),
                    value: "param".to_string(),
                },
            ),
            (
                " -- comment ",
                ParsedLine {
                    line_type: LineType::Comment,
                    tag: String::new(),
                    value: "comment".to_string(),
                },
            ),
            (
                " --",
                ParsedLine {
                    line_type: LineType::Comment,
                    tag: String::new(),
                    value: String::new(),
                },
            ),
        ];

        for (input, expected) in tests {
            let parsed = parse_line(input);
            assert_eq!(parsed.line_type, expected.line_type);
            assert_eq!(parsed.tag, expected.tag);
            assert_eq!(parsed.value, expected.value);
        }
    }

    #[test]
    fn test_scanner_err_tags() {
        let double = r#"
-- name: first
-- name: clone
SELECT * FROM foo;
"#;

        let missing = r#"
SELECT * FROM missing;
"#;

        for (key, content) in [("double", double), ("missing", missing)] {
            let result = parse(content.as_bytes());
            assert!(result.is_err(), "expected error for {}, but got Ok", key);
        }
    }

    #[test]
    fn test_scanner_valid() {
        let valid_sql = r#"
-- name: simple
-- raw: 1
SELECT * FROM simple;
-- name: multiline
SELECT *
FROM multiline
WHERE line = 42;
-- name: comments
-- yoyo
SELECT *
-- inline
FROM comments;
"#;

        let queries = parse(valid_sql.as_bytes()).unwrap();

        let expected_queries: HashMap<&str, &str> = [
            ("simple", "SELECT * FROM simple;"),
            ("multiline", "SELECT * FROM multiline WHERE line = 42;"),
            ("comments", "SELECT * FROM comments;"),
        ]
        .iter()
        .cloned()
        .collect();

        assert_eq!(queries.len(), expected_queries.len());

        assert_eq!(queries["simple"].tags.get("raw"), Some(&String::from("1")));

        for (key, expected_query) in expected_queries {
            assert_eq!(queries[key].query.trim(), expected_query);
        }
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse_file("tests/samples/missing.sql");
        assert!(!result.is_ok());
    }

    #[test]
    fn test_parse_file() {
        let result = parse_file("test.sql");
        assert!(result.is_ok(), "error parsing file: {:?}", result.err());

        let queries = result.unwrap();

        assert_eq!(
            queries["simple"].tags.get("first"),
            Some(&String::from("yes"))
        );
        assert_eq!(
            queries["simple"].query,
            String::from("SELECT * FROM simple;")
        );
        assert_eq!(
            queries["simple2"].query,
            String::from("SELECT * FROM simple2;")
        );
    }

    #[test]
    fn test_parse_invalid_bytes() {
        let result = parse("this will fail".as_bytes());
        assert!(!result.is_ok());
    }

    #[test]
    fn test_parse_bytes() {
        let result = parse(
            "--name: simple\nSELECT * FROM simple;\n--name: simple2\nSELECT * FROM simple2;"
                .as_bytes(),
        );
        assert!(result.is_ok());
    }
}
