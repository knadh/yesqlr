#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_bytes() {
        let result = yesqlr::parse(
            "--name: simple\nSELECT * FROM simple;\n--name: simple2\nSELECT * FROM simple2;"
                .as_bytes(),
        );
        assert!(result.is_ok());

        #[derive(Default, yesqlr_macros::ScanQueries)]
        struct Q {
            simple: yesqlr::Query,

            #[name = "simple2"]
            simple_two: yesqlr::Query,

            another: yesqlr::Query,
        }

        let q: Q = Q::try_from(result.unwrap()).expect("errong converting queries to Q");

        assert_eq!(q.simple.query, "SELECT * FROM simple;");
        assert_eq!(q.simple_two.query, "SELECT * FROM simple2;");
        assert_eq!(q.another.query, "");
    }
}
