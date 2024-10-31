# yesqlr

yesqlr is a Rust port of the [goyesql](https://github.com/knadh/goyesql) Go library.
It allows multiple SQL queries to be defined in an `.sql` file, each separated by a specially formatted `--name: $name`
accompanying every query, which the library then parses to a HashMap<$name, Query{}>.
In addition, it also supports attaching arbitrary --$key: $value tags with every query
This allows better organization and handling of SQL code in Rust projects.


## Usage

Create a `.sql` file with multiple queries, each preceded by a `-- name: query_name` tag. Additional tags can be added as needed.

```sql
-- name: get_user
-- raw: true
SELECT * FROM users WHERE id = $1;

-- name: create_user
INSERT INTO users (name, email) VALUES ($1, $2);
```


### Parsing SQL files

Use the `parse_file()` function to read and parse the `.sql` file.

```rust
use yesqlr::parse_file;

fn main() -> Result<(), yesqlr::ParseError> {
    let queries = parse_file("test.sql").expect("error parsing file");
    let q = &queries["simple"].query;
    println!("the query is: {}", q);
    Ok(())
}
```


### Parsing bytes / Reader

Alternatively, parse SQL queries from a byte stream using the `parse()` function.

```rust
use yesqlr::parse;

fn main() -> Result<(), yesqlr::ParseError> {
    let raq = b"-- name: list_users\nSELECT * FROM users;";
    let queries = parse(&raq[..])?;
    let list_users_query = &queries["list_users"].query;
    println!("user query is: {}", list_users_query);
    Ok(())
}
```

### Parsing into a struct

```rust
use yesqlr::parse;

fn main() -> Result<(), yesqlr::ParseError> {
    // Parse queries from bytes or file first.
    let result = parse("--name: simple\nSELECT * FROM simple;\n--name: simple2\nSELECT * FROM simple2;").as_bytes();

    // Define the struct. 'name' can be overridden.
    #[derive(Default, ScanQueries)]
    struct Q {
        simple: Query,

        #[name = "simple2"]
        simple_two: Query,

        another: Query,
    }

    let q: Q = Q::try_from(result.unwrap()).expect("Failed to convert queries to Q");
}
```


## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
