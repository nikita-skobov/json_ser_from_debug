/// A minimal json serializer for arbitrary structs that implement Debug.
/// Limitations:
/// - must be struct at the root level, ie: this is only capable of emitting
///   json that is an object {} at the root
/// - Does not support enums at all
///   this includes standard library enums like Result, Option, as well as user defined enums.
///   Therefore, your struct must not contain any enums
/// - Any field of a struct must not start with a capital letter
/// Use cases:
/// Because the limitations are quite severe, this has a minimal use case.
/// This is only really useful for scenarios where you want json serialization
/// for simple structures that are always objects, and when you don't want to
/// add a heavy dependency like serde.
/// # Example:
/// ```
/// use json_ser_from_debug::json_ser;
/// #[derive(Debug)]
/// pub struct MyData {
///     pub a: String,
///     pub b: String,
/// }
/// 
/// let json_string = json_ser::serialize(&MyData { a: "hello".to_string(), b: "world".to_string() });
/// assert_eq!(json_string, r#"{"a":"hello","b":"world"}"#);
/// ```
pub mod json_ser {
    use std::fmt::{Write, Debug};

    pub fn serialize(obj: &dyn Debug) -> String {
        serialize_with_renamed_fields(obj, keep_as_is)
    }

    pub fn serialize_with_pascal_case(obj: &dyn Debug) -> String {
        serialize_with_renamed_fields(obj, pascal_case)
    }

    /// optionally provide a callback that will be called every time we insert a new field.
    /// This lets you rename fields according to your custom semantics such as changing the casing.
    /// For default pascal case renaming, see `serialize_with_pascal_case`.
    pub fn serialize_with_renamed_fields(obj: &dyn Debug, rename_fn: fn(&str) -> String) -> String {
        let mut agg = JsonCommandAggregator {
            current: "".to_string(),
            expecting: OPEN_BRACE,
            rename_field: rename_fn,
        };
        // this never fails
        let _ = std::fmt::write(&mut agg, format_args!("{:#?}", obj));
        agg.current
    }

    const OPEN_BRACE:          u16 = 0b1000_0000_0000_0000;
    const CLOSE_BRACE:         u16 = 0b0100_0000_0000_0000;
    const OPEN_BRACKET:        u16 = 0b0010_0000_0000_0000;
    const CLOSE_BRACKET:       u16 = 0b0001_0000_0000_0000;
    const COLON:               u16 = 0b0000_1000_0000_0000;
    const START_QUOTE:         u16 = 0b0000_0100_0000_0000;
    const END_QUOTE:           u16 = 0b0000_0010_0000_0000;
    const STRING:              u16 = 0b0000_0001_0000_0000;
    const ESCAPE_CHAR:         u16 = 0b0000_0000_1000_0000;
    const FIELD_NAME:          u16 = 0b0000_0000_0100_0000;
    const NUMBER:              u16 = 0b0000_0000_0010_0000;

    fn keep_as_is(s: &str) -> String {
        s.to_string()
    }

    fn pascal_case(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut first = true;
        let mut last_was_underscore = false;
        for c in s.chars() {
            if first {
                out.push(c.to_ascii_uppercase());
                first = false;
                continue;
            }
            if c == '_' {
                last_was_underscore = true;
                continue;
            }
            // if the last char was an underscore, then capitalize this one
            if last_was_underscore {
                out.push(c.to_ascii_uppercase());
                last_was_underscore = false;
                continue;
            }
            out.push(c);
        }
        out
    }

    struct JsonCommandAggregator {
        current: String,
        expecting: u16,
        rename_field: fn(&str) -> String,
    }

    #[inline(always)]
    fn flag_has(val: u16, flag: u16) -> bool {
        val & flag != 0
    }

    // our strategy for adding trailing commas is simply to
    // look at our current json string, and if the last thing we see
    // is either an object, list, or string being closed, or a number,
    // or true/false then
    // we know we need to add a comma before adding the next
    // - field
    // - object
    // - or list
    fn add_comma(s: &mut String) {
        if let Some(c) = s.chars().last() {
            if c == '"' || c == ']' || c == '}' || c.is_ascii_digit() || c == 'e' {
                s.push_str(",");
            }
        }
    }

    fn fieldname_does_not_start_with_capital(n: &str) -> bool {
        if let Some(c) = n.chars().nth(0) {
            return !c.is_ascii_uppercase();
        }
        true
    }

    // the Write trait is used whenever you do something like
    // println!("{:?}", obj);
    // with an object that implements Debug.
    // We take advantage of this and make a custom implementation
    // that receives every string that the Debug trait would pass to the writer
    // and parse out just the parts that look json-ish
    // and append our json string.
    impl Write for JsonCommandAggregator {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let s = s.trim();
            match (s, self.expecting) {
                ("{", x) if flag_has(x, OPEN_BRACE) => {
                    add_comma(&mut self.current);
                    self.current.push_str("{");
                    self.expecting = FIELD_NAME | CLOSE_BRACE;
                }
                ("}", x) if flag_has(x, CLOSE_BRACE) => {
                    self.current.push_str("}");
                    self.expecting = FIELD_NAME | CLOSE_BRACE | CLOSE_BRACKET | START_QUOTE | OPEN_BRACE | OPEN_BRACKET;
                }
                ("[" | "(", x) if flag_has(x, OPEN_BRACKET) => {
                    add_comma(&mut self.current);
                    self.current.push_str("[");
                    self.expecting = START_QUOTE | OPEN_BRACE | OPEN_BRACKET | CLOSE_BRACKET | NUMBER;
                }
                ("]" | ")", x) if flag_has(x, CLOSE_BRACKET) => {
                    self.current.push_str("]");
                    self.expecting = FIELD_NAME | CLOSE_BRACE | CLOSE_BRACKET | START_QUOTE | OPEN_BRACE | OPEN_BRACKET;
                }
                (":", x) if flag_has(x, COLON) => {
                    self.expecting = START_QUOTE | OPEN_BRACE | OPEN_BRACKET | NUMBER;
                }
                ("\"", x) if flag_has(x, START_QUOTE) => {
                    add_comma(&mut self.current);
                    self.current.push('"');
                    self.expecting = END_QUOTE | STRING | ESCAPE_CHAR;
                }
                ("\"", x) if flag_has(x, END_QUOTE) => {
                    self.current.push('"');
                    self.expecting = START_QUOTE | FIELD_NAME | CLOSE_BRACE | CLOSE_BRACKET | OPEN_BRACE | OPEN_BRACKET;
                }
                ("true", x) if flag_has(x, NUMBER) => {
                    self.current.push_str("true");
                    self.expecting = FIELD_NAME | CLOSE_BRACE | CLOSE_BRACKET | NUMBER | START_QUOTE;
                }
                ("false", x) if flag_has(x, NUMBER) => {
                    self.current.push_str("false");
                    self.expecting = FIELD_NAME | CLOSE_BRACE | CLOSE_BRACKET | NUMBER | START_QUOTE;
                }
                (num, x) if flag_has(x, NUMBER) && num.parse::<f64>().is_ok() => {
                    self.current.push_str(num);
                    self.expecting = FIELD_NAME | CLOSE_BRACE | CLOSE_BRACKET | NUMBER | START_QUOTE;
                }
                // field name and string conflict.
                (val, x) if flag_has(x, STRING) => {
                    if val == "\\" {
                        self.current.push_str("\\\\");
                        self.expecting = STRING;
                    } else {
                        self.current.push_str(val);
                        self.expecting = END_QUOTE | STRING | ESCAPE_CHAR;
                    }
                }
                (field_name, x) if flag_has(x, FIELD_NAME) && fieldname_does_not_start_with_capital(field_name) => {
                    if field_name.is_empty() { return std::fmt::Result::Ok(()) }
                    if field_name == "," { return std::fmt::Result::Ok(()) }
                    add_comma(&mut self.current);
                    self.current.push_str("\"");
                    self.current.push_str(&(self.rename_field)(field_name));
                    self.current.push_str("\"");
                    self.current.push_str(":");
                    self.expecting = COLON;
                }
                _ => {}
            }
            std::fmt::Result::Ok(())
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    pub struct Basic {
        pub hello: String,
    }

    #[derive(Debug)]
    pub struct Basic2 {
        pub hello_world: String,
    }

    #[derive(Debug)]
    pub struct T1 {
        pub bool1: bool,
        pub middle: String,
        pub bool2: bool,
        pub after: String,
    }

    #[derive(Debug)]
    pub struct Nested {
        pub nest: Basic,
    }

    #[derive(Debug)]
    pub struct Lists {
        pub l1: Vec<T1>,
        pub l2: (Vec<T1>, String, T1, Vec<String>),
    }

    #[derive(Debug)]
    pub struct Tuples {
        pub t: (Vec<Basic>, String, Basic),
    }

    #[derive(Debug)]
    pub enum Ee {
        Variant1(String),
    }

    #[test]
    fn basic_works() {
        let obj = Basic { hello: "world".to_string() };
        let json_str = json_ser::serialize(&obj);
        assert_eq!(json_str, r#"{"hello":"world"}"#);
    }

    #[test]
    fn escaping_works() {
        let obj = Basic { hello: "\"".to_string() };
        let json_str = json_ser::serialize(&obj);
        assert_eq!(json_str, r#"{"hello":"\\""}"#);
    }

    #[test]
    fn pascal_renaming_works() {
        let obj = Basic2 { hello_world: "yes".to_string() };
        let json_str = json_ser::serialize_with_pascal_case(&obj);
        assert_eq!(json_str, r#"{"HelloWorld":"yes"}"#);
    }

    #[test]
    fn bools_work() {
        let obj = T1 { bool1: true, middle: "hi".to_string(), bool2: false, after: "world".to_string() };
        let json_str = json_ser::serialize(&obj);
        assert_eq!(json_str, r#"{"bool1":true,"middle":"hi","bool2":false,"after":"world"}"#);
    }

    #[test]
    fn tuples_work() {
        let obj = Tuples {
            t: (vec![], "a".to_string(), Basic { hello: "world".to_string()})
        };
        let json_str = json_ser::serialize(&obj);
        assert_eq!(json_str, r#"{"t":[[],"a",{"hello":"world"}]}"#);
    }

    #[test]
    fn lists_work() {
        let obj = Lists {
            l1: vec![
                T1 { bool1: true, middle: "".to_string(), bool2: true, after: "".to_string()},
                T1 { bool1: false, middle: "".to_string(), bool2: false, after: "".to_string()},
            ],
            l2: (
                vec![
                    T1 { bool1: true, middle: "".to_string(), bool2: true, after: "".to_string()},
                    T1 { bool1: false, middle: "".to_string(), bool2: false, after: "".to_string()},
                ],
                "a".to_string(),
                T1 { bool1: true, middle: "hi".to_string(), bool2: false, after: "world".to_string()},
                vec!["x".to_string(), "y".to_string(), "z".to_string()],
            )
        };
        let json_str = json_ser::serialize(&obj);
        assert_eq!(json_str, r#"{"l1":[{"bool1":true,"middle":"","bool2":true,"after":""},{"bool1":false,"middle":"","bool2":false,"after":""}],"l2":[[{"bool1":true,"middle":"","bool2":true,"after":""},{"bool1":false,"middle":"","bool2":false,"after":""}],"a",{"bool1":true,"middle":"hi","bool2":false,"after":"world"},["x","y","z"]]}"#);
    }

    #[test]
    fn nested_works() {
        let obj = Nested {
            nest: Basic { hello: "world".to_string() }
        };
        let json_str = json_ser::serialize(&obj);
        assert_eq!(json_str, r#"{"nest":{"hello":"world"}}"#);
    }

    #[test]
    #[should_panic]
    fn enums_dont_work() {
        let ee = Ee::Variant1("hi".to_string());
        let json_str = json_ser::serialize(&ee);
        assert!(!json_str.is_empty());
    }
}
