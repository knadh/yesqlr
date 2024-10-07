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
const TAG_END: &str = "end";

lazy_static! {
    static ref RE_TAG: Regex = Regex::new(r"^\s*--\s*(\w+)\s*:\s*(.+)").unwrap();
    static ref RE_COMMENT: Regex = Regex::new(r"^\s*--\s*(.*)").unwrap();
    static ref RE_PLACEHOLDER: Regex = Regex::new(r"\$\d+").unwrap(); // Match placeholders like $1, $2.
}


// Represents an parse error.
#[derive(Debug)]
pub enum ParseError {
    IOError(String),
    DuplicateTag(String),
    MissingNameTag(String),
    EmptyQuery(String),
    InvalidTagOrder(String),
    UnmatchedPlaceholders(String),
    MalformedSQL(String),
}


impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::IOError(e) => write!(f, "IO error: {}", e),
            ParseError::DuplicateTag(e) => write!(f, "Duplicate tag: {}", e),
            ParseError::MissingNameTag(e) => write!(f, "Missing name tag: {}", e),
            ParseError::EmptyQuery(e) => write!(f, "Query without content: {}", e),
            ParseError::InvalidTagOrder(e) => write!(f, "Invalid tag order: {}", e),
            ParseError::UnmatchedPlaceholders(e) => write!(f, "Unmatched placeholders: {}", e),
            ParseError::MalformedSQL(e) => write!(f, "Malformed SQL syntax: {}", e),
        }
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
    EndTag, // to handle multi-statement SQL blocks
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
    let file = File::open(&path).map_err(|e| ParseError::IOError(format!("Error reading file '{}': {}", path.as_ref().display(), e)))?;
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
        let line = line.map_err(|e| ParseError::IOError(format!("Error reading line {}: {}", i + 1, e)))?;
        let parsed_line = parse_line(&line);

        match parsed_line.line_type {
            LineType::Blank | LineType::Comment => continue,
            LineType::Query => {
                if name.is_empty() {
                    return Err(ParseError::MissingNameTag(format!(
                        "Query without 'name' tag found at line {}: '{}'",
                        i + 1, parsed_line.value
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
                        return Err(ParseError::DuplicateTag(format!(
                            "Duplicate query name found: '{}'",
                            name
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
                        return Err(ParseError::InvalidTagOrder(format!(
                            "'name' should be the first tag, found '{}' at line {}",
                            parsed_line.tag, i + 1
                        )));
                    }
                    let q = queries.get_mut(&name).unwrap();
                    if q.tags.contains_key(&parsed_line.tag) {
                        return Err(ParseError::DuplicateTag(format!(
                            "Duplicate tag '{}' for query '{}'",
                            parsed_line.tag, name
                        )));
                    }
                    q.tags.insert(parsed_line.tag, parsed_line.value);
                }
            }
            LineType::EndTag => {
                // Handle the end of a multi-statement query block.
                name.clear();
            }
        }
    }

    // Post-validation of all queries to ensure they have content and correct placeholder usage.
    for (name, query) in &queries {
        if query.query.is_empty() {
            return Err(ParseError::EmptyQuery(format!("Query '{}' is empty", name)));
        }
    
        // Check for correct placeholder sequence.
        let placeholders: Vec<usize> = RE_PLACEHOLDER.find_iter(&query.query)
            .map(|m| m.as_str()[1..].parse::<usize>().unwrap()) // Get the numeric part of placeholders like $1.
            .collect();
        
        // Ensure the placeholders are in a proper sequence without duplicates or gaps.
        for (i, &num) in placeholders.iter().enumerate() {
            if num != i + 1 {
                return Err(ParseError::UnmatchedPlaceholders(format!(
                    "Query '{}' has incorrect placeholder order: expected {}, found {}",
                    name, i + 1, num
                )));
            }
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

    // Check if the line is an end tag
    if line.starts_with("-- end") {
        return ParsedLine {
            line_type: LineType::EndTag,
            tag: TAG_END.to_string(),
            value: String::new(), // Keep value empty as `-- end` does not have associated content.
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
        // Testing various line types.
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
                " -- end ",
                ParsedLine {
                    line_type: LineType::EndTag,
                    tag: "end".to_string(),
                    value: String::new(),
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
    fn test_parse_bytes_no_panic() {
        let result = parse("-- name: byte-me\nSELECT * FROM bytes;".as_bytes());
        assert!(result.is_ok());
    }

    #[test]
    fn test_placeholder_validation() {
        // Testing for consistent placeholder usage.
        let sql = r#"
        -- name: valid_query
        SELECT * FROM users WHERE id = $1;
        "#;

        let queries = parse(sql.as_bytes());
        assert!(queries.is_ok(), "Placeholder check failed for valid query.");

        let invalid_sql = r#"
        -- name: invalid_query
        SELECT * FROM users WHERE id = $1 AND email = $2 AND age = $1;
        "#;

        let queries = parse(invalid_sql.as_bytes());
        assert!(queries.is_err(), "Expected error for inconsistent placeholders.");
    }

    #[test]
    fn test_multi_statement_query() {
        // Testing multi-statement support.
        let multi_stmt_sql = r#"
        -- name: transaction_block
        BEGIN;
        INSERT INTO users (name, email) VALUES ($1, $2);
        COMMIT;
        -- end;
        "#;

        let queries = parse(multi_stmt_sql.as_bytes()).expect("Failed to parse multi-statement query.");
        let query = &queries["transaction_block"].query;
        assert_eq!(query, "BEGIN; INSERT INTO users (name, email) VALUES ($1, $2); COMMIT;");
    }

    #[test]
fn test_parse_error_conditions() {
    // Test duplicate tag error
    let duplicate_tag_sql = r#"
    -- name: duplicate_query
    SELECT * FROM users;
    -- name: duplicate_query
    SELECT * FROM users;
    "#;
    let result = parse(duplicate_tag_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::DuplicateTag(_))), "Expected DuplicateTag error");

    // Test missing name tag error
    let missing_name_tag_sql = r#"
    SELECT * FROM users;
    "#;
    let result = parse(missing_name_tag_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::MissingNameTag(_))), "Expected MissingNameTag error");

    // Test invalid tag order
    let invalid_tag_order_sql = r#"
    -- raw: true
    SELECT * FROM users;
    -- name: invalid_order_query
    "#;
    let result = parse(invalid_tag_order_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::InvalidTagOrder(_))), "Expected InvalidTagOrder error");

    // Test empty query
    let empty_query_sql = r#"
    -- name: empty_query
    -- end;
    "#;
    let result = parse(empty_query_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::EmptyQuery(_))), "Expected EmptyQuery error");
}

#[test]
fn test_placeholder_sequence_validation() {
    // Test valid sequence of placeholders
    let valid_placeholder_sql = r#"
    -- name: valid_sequence_query
    SELECT * FROM users WHERE id = $1 AND email = $2;
    "#;
    let result = parse(valid_placeholder_sql.as_bytes());
    assert!(result.is_ok(), "Expected valid sequence of placeholders");

    // Test invalid placeholder sequence
    let invalid_placeholder_sequence_sql = r#"
    -- name: invalid_sequence_query
    SELECT * FROM users WHERE id = $1 AND email = $3;
    "#;
    let result = parse(invalid_placeholder_sequence_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::UnmatchedPlaceholders(_))), "Expected UnmatchedPlaceholders error for sequence");

    // Test duplicate placeholder usage
    let duplicate_placeholder_sql = r#"
    -- name: duplicate_placeholder_query
    SELECT * FROM users WHERE id = $1 AND email = $1;
    "#;
    let result = parse(duplicate_placeholder_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::UnmatchedPlaceholders(_))), "Expected UnmatchedPlaceholders error for duplicates");
}

#[test]
fn test_multi_statement_query_parsing() {
    let multi_stmt_sql = r#"
    -- name: transaction_block
    BEGIN;
    INSERT INTO users (name, email) VALUES ($1, $2);
    COMMIT;
    -- end;
    "#;

    let queries = parse(multi_stmt_sql.as_bytes()).expect("Failed to parse multi-statement query");
    let query = &queries["transaction_block"].query;

    assert_eq!(query, "BEGIN; INSERT INTO users (name, email) VALUES ($1, $2); COMMIT;");
}

#[test]
fn test_tag_validation() {
    // Test proper tag order
    let proper_tag_order_sql = r#"
    -- name: proper_order_query
    -- raw: true
    SELECT * FROM users;
    "#;
    let result = parse(proper_tag_order_sql.as_bytes());
    assert!(result.is_ok(), "Expected proper tag order to parse successfully");

    // Test duplicate tag
    let duplicate_tag_sql = r#"
    -- name: duplicate_tag_query
    -- raw: true
    -- raw: true
    SELECT * FROM users;
    "#;
    let result = parse(duplicate_tag_sql.as_bytes());
    assert!(matches!(result, Err(ParseError::DuplicateTag(_))), "Expected DuplicateTag error");
}



}
