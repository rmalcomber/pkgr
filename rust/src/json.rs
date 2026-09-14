//! A targeted JSONC scanner.
//!
//! A general-purpose JSON library would build a full document tree, which needs
//! an order-preserving map and a value enum covering every type. pkgr only ever
//! wants two things out of a manifest: one named object of tasks, and one
//! optional string. So this walks the top level, keeps those, and skips
//! everything else without allocating for it.
//!
//! Source order falls out for free: tasks are pushed as they are encountered,
//! so there is no need for an order-preserving map at all.
//!
//! Comments and trailing commas are handled inline, which Deno permits in
//! deno.json as well as deno.jsonc.

use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Task {
    pub name: String,
    pub command: String,
    pub description: String,
}

#[derive(Debug, PartialEq)]
pub struct Manifest {
    pub tasks: Vec<Task>,
    /// Raw `packageManager` field, e.g. "pnpm@9.1.0". Empty when absent.
    pub package_manager: String,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Malformed JSON, with a byte offset to point at.
    Syntax { at: usize, msg: String },
    /// Parsed cleanly, but there is nothing runnable in it.
    NoTasks,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax { at, msg } => write!(f, "{msg} at byte {at}"),
            Error::NoTasks => write!(f, "no scripts or tasks declared"),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

/// Reads `src`, collecting the tasks under `task_key` ("scripts" or "tasks")
/// plus the `packageManager` field.
pub fn parse(src: &str, task_key: &str) -> Result<Manifest> {
    let mut s = Scanner::new(src.as_bytes());
    let mut out = Manifest {
        tasks: Vec::new(),
        package_manager: String::new(),
    };

    s.skip_trivia();
    s.expect(b'{')?;

    loop {
        s.skip_trivia();
        if s.eat(b'}') {
            break;
        }

        let key = s.parse_string()?;
        s.skip_trivia();
        s.expect(b':')?;
        s.skip_trivia();

        if key == task_key {
            out.tasks = s.parse_tasks()?;
        } else if key == "packageManager" {
            // A non-string here is odd but survivable: lockfile detection is
            // still available, so skip it rather than failing the whole parse.
            if s.peek() == Some(b'"') {
                out.package_manager = s.parse_string()?;
            } else {
                s.skip_value()?;
            }
        } else {
            s.skip_value()?;
        }

        s.skip_trivia();
        if !s.eat(b',') {
            s.skip_trivia();
            s.expect(b'}')?;
            break;
        }
    }

    if out.tasks.is_empty() {
        return Err(Error::NoTasks);
    }
    Ok(out)
}

struct Scanner<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(b: &'a [u8]) -> Self {
        Scanner { b, i: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn eat(&mut self, want: u8) -> bool {
        if self.peek() == Some(want) {
            self.i += 1;
            return true;
        }
        false
    }

    fn expect(&mut self, want: u8) -> Result<()> {
        if self.eat(want) {
            return Ok(());
        }
        Err(self.err(format!("expected {:?}", want as char)))
    }

    fn err(&self, msg: String) -> Error {
        Error::Syntax { at: self.i, msg }
    }

    /// Skips whitespace and comments. JSON proper has no comments, but Deno
    /// accepts them, so they are treated as whitespace here.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => self.i += 1,
                Some(b'/') => match self.b.get(self.i + 1) {
                    Some(b'/') => {
                        while let Some(c) = self.peek() {
                            if c == b'\n' {
                                break;
                            }
                            self.i += 1;
                        }
                    }
                    Some(b'*') => {
                        self.i += 2;
                        while self.i < self.b.len() {
                            if self.b[self.i] == b'*' && self.b.get(self.i + 1) == Some(&b'/') {
                                self.i += 2;
                                break;
                            }
                            self.i += 1;
                        }
                    }
                    _ => return,
                },
                _ => return,
            }
        }
    }

    /// Parses a JSON string, resolving escapes.
    fn parse_string(&mut self) -> Result<String> {
        self.expect(b'"')?;
        let mut out = String::new();

        loop {
            let c = self
                .peek()
                .ok_or_else(|| self.err("unterminated string".into()))?;
            self.i += 1;

            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self
                        .peek()
                        .ok_or_else(|| self.err("unterminated escape".into()))?;
                    self.i += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.parse_unicode_escape()?),
                        other => {
                            return Err(self.err(format!("bad escape {:?}", other as char)));
                        }
                    }
                }
                // Multi-byte UTF-8 is copied through a byte at a time; the
                // input was already validated as UTF-8 by read_to_string.
                _ => {
                    let start = self.i - 1;
                    let len = utf8_len(c);
                    self.i = start + len;
                    match std::str::from_utf8(&self.b[start..self.i.min(self.b.len())]) {
                        Ok(s) => out.push_str(s),
                        Err(_) => return Err(self.err("invalid UTF-8 in string".into())),
                    }
                }
            }
        }
    }

    /// Handles \uXXXX, including surrogate pairs for characters outside the BMP.
    fn parse_unicode_escape(&mut self) -> Result<char> {
        let first = self.parse_hex4()?;

        // A high surrogate is only half a character; the low half follows.
        if (0xD800..0xDC00).contains(&first) {
            if self.peek() == Some(b'\\') && self.b.get(self.i + 1) == Some(&b'u') {
                self.i += 2;
                let second = self.parse_hex4()?;
                if (0xDC00..0xE000).contains(&second) {
                    let combined = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                    return char::from_u32(combined)
                        .ok_or_else(|| self.err("invalid surrogate pair".into()));
                }
            }
            return Err(self.err("unpaired high surrogate".into()));
        }

        char::from_u32(first).ok_or_else(|| self.err("invalid code point".into()))
    }

    fn parse_hex4(&mut self) -> Result<u32> {
        let mut n = 0u32;
        for _ in 0..4 {
            let c = self
                .peek()
                .ok_or_else(|| self.err("short \\u escape".into()))?;
            let d = match c {
                b'0'..=b'9' => (c - b'0') as u32,
                b'a'..=b'f' => (c - b'a') as u32 + 10,
                b'A'..=b'F' => (c - b'A') as u32 + 10,
                _ => return Err(self.err("bad hex digit".into())),
            };
            n = n * 16 + d;
            self.i += 1;
        }
        Ok(n)
    }

    /// Consumes any value without interpreting it.
    fn skip_value(&mut self) -> Result<()> {
        self.skip_trivia();
        match self.peek() {
            Some(b'"') => {
                self.parse_string()?;
                Ok(())
            }
            Some(b'{') | Some(b'[') => self.skip_container(),
            None => Err(self.err("unexpected end of input".into())),
            // Numbers, true, false, null: run to the next structural byte.
            _ => {
                let start = self.i;
                while let Some(c) = self.peek() {
                    if matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n' | b'/') {
                        break;
                    }
                    self.i += 1;
                }
                if self.i == start {
                    // Consuming nothing means there was no value here at all,
                    // e.g. `{"a":}`. Left unchecked this would silently look
                    // like an absent key rather than malformed input.
                    return Err(self.err("expected a value".into()));
                }
                Ok(())
            }
        }
    }

    /// Skips a balanced object or array, ignoring structural bytes that appear
    /// inside strings or comments.
    fn skip_container(&mut self) -> Result<()> {
        let mut depth = 0usize;
        loop {
            self.skip_trivia();
            let c = self
                .peek()
                .ok_or_else(|| self.err("unterminated object or array".into()))?;

            match c {
                b'{' | b'[' => {
                    depth += 1;
                    self.i += 1;
                }
                b'}' | b']' => {
                    depth -= 1;
                    self.i += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                b'"' => {
                    self.parse_string()?;
                }
                _ => self.i += 1,
            }
        }
    }

    /// Parses the task object: `{ "name": "command" | { ... }, ... }`.
    fn parse_tasks(&mut self) -> Result<Vec<Task>> {
        self.skip_trivia();
        self.expect(b'{')?;

        let mut tasks = Vec::new();
        loop {
            self.skip_trivia();
            if self.eat(b'}') {
                return Ok(tasks);
            }

            let name = self.parse_string()?;
            self.skip_trivia();
            self.expect(b':')?;
            self.skip_trivia();

            match self.peek() {
                Some(b'"') => {
                    let command = self.parse_string()?;
                    tasks.push(Task {
                        name,
                        command,
                        description: String::new(),
                    });
                }
                Some(b'{') => tasks.push(self.parse_task_object(name)?),
                // Anything else is not runnable; ignore it rather than failing.
                _ => self.skip_value()?,
            }

            self.skip_trivia();
            if !self.eat(b',') {
                self.skip_trivia();
                self.expect(b'}')?;
                return Ok(tasks);
            }
        }
    }

    /// Deno's object form: a command, an optional description, and optional
    /// task dependencies.
    fn parse_task_object(&mut self, name: String) -> Result<Task> {
        self.expect(b'{')?;

        let mut command = String::new();
        let mut description = String::new();
        let mut dependencies: Vec<String> = Vec::new();

        loop {
            self.skip_trivia();
            if self.eat(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.skip_trivia();
            self.expect(b':')?;
            self.skip_trivia();

            match key.as_str() {
                "command" if self.peek() == Some(b'"') => command = self.parse_string()?,
                "description" if self.peek() == Some(b'"') => description = self.parse_string()?,
                "dependencies" if self.peek() == Some(b'[') => {
                    self.i += 1;
                    loop {
                        self.skip_trivia();
                        if self.eat(b']') {
                            break;
                        }
                        if self.peek() == Some(b'"') {
                            dependencies.push(self.parse_string()?);
                        } else {
                            self.skip_value()?;
                        }
                        self.skip_trivia();
                        if !self.eat(b',') {
                            self.skip_trivia();
                            self.expect(b']')?;
                            break;
                        }
                    }
                }
                _ => self.skip_value()?,
            }

            self.skip_trivia();
            if !self.eat(b',') {
                self.skip_trivia();
                self.expect(b'}')?;
                break;
            }
        }

        // A dependencies-only task is still runnable, so give it a readable row.
        if command.is_empty() && !dependencies.is_empty() {
            command = format!("depends on: {}", dependencies.join(", "));
        }

        Ok(Task {
            name,
            command,
            description,
        })
    }
}

/// Length in bytes of a UTF-8 sequence from its leading byte.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(m: &Manifest) -> Vec<&str> {
        m.tasks.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn preserves_source_order() {
        let src = r#"{
            "name": "demo",
            "scripts": { "zebra": "z", "build": "b", "alpha": "a" },
            "devDependencies": { "nested": { "deep": [1, 2, {"x": true}] } }
        }"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(names(&m), ["zebra", "build", "alpha"]);
        assert_eq!(m.tasks[1].command, "b");
    }

    #[test]
    fn reads_package_manager() {
        let src = r#"{"packageManager":"pnpm@9.1.0","scripts":{"dev":"vite"}}"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(m.package_manager, "pnpm@9.1.0");
    }

    #[test]
    fn tolerates_non_string_package_manager() {
        let src = r#"{"packageManager":123,"scripts":{"dev":"vite"}}"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(m.package_manager, "");
        assert_eq!(names(&m), ["dev"]);
    }

    #[test]
    fn deno_object_tasks() {
        let src = r#"{
            "tasks": {
                "build": { "description": "Bundle it", "command": "deno bundle main.ts" },
                "all": { "dependencies": ["build", "test"] },
                "test": "deno test -A"
            }
        }"#;
        let m = parse(src, "tasks").unwrap();
        assert_eq!(names(&m), ["build", "all", "test"]);
        assert_eq!(m.tasks[0].command, "deno bundle main.ts");
        assert_eq!(m.tasks[0].description, "Bundle it");
        assert_eq!(m.tasks[1].command, "depends on: build, test");
    }

    #[test]
    fn comments_and_trailing_commas() {
        let src = "{\n // leading\n \"tasks\": {\n \"dev\": \"deno run -A main.ts\", // inline\n /* block\n comment */\n \"fmt\": \"deno fmt\",\n },\n}";
        let m = parse(src, "tasks").unwrap();
        assert_eq!(names(&m), ["dev", "fmt"]);
        assert_eq!(m.tasks[0].command, "deno run -A main.ts");
    }

    #[test]
    fn structural_bytes_inside_strings_are_data() {
        let src = r#"{"homepage":"https://x.com//docs","scripts":{"a":"echo }{,"},"z":1}"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(names(&m), ["a"]);
        assert_eq!(m.tasks[0].command, "echo }{,");
    }

    #[test]
    fn string_escapes() {
        let src = r#"{"scripts":{"a":"say \"hi\"\n\tdone \\ A 😀"}}"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(m.tasks[0].command, "say \"hi\"\n\tdone \\ A 😀");
    }

    #[test]
    fn multibyte_utf8_passes_through() {
        let src = r#"{"scripts":{"i18n":"echo héllo → 世界"}}"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(m.tasks[0].command, "echo héllo → 世界");
    }

    #[test]
    fn missing_key_reports_no_tasks() {
        let src = r#"{"name":"x","dependencies":{}}"#;
        assert_eq!(parse(src, "scripts").unwrap_err(), Error::NoTasks);
    }

    #[test]
    fn empty_task_object_reports_no_tasks() {
        assert_eq!(
            parse(r#"{"scripts":{}}"#, "scripts").unwrap_err(),
            Error::NoTasks
        );
    }

    #[test]
    fn wrong_key_for_kind_is_not_found() {
        // A deno.json read as npm must not pick up its tasks.
        assert_eq!(
            parse(r#"{"tasks":{"a":"b"}}"#, "scripts").unwrap_err(),
            Error::NoTasks
        );
    }

    #[test]
    fn syntax_errors_carry_an_offset() {
        match parse(r#"{"scripts":{"a":}}"#, "scripts") {
            Err(Error::Syntax { at, .. }) => assert!(at > 0),
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn non_object_top_level_is_an_error() {
        assert!(matches!(
            parse("[1,2,3]", "scripts"),
            Err(Error::Syntax { .. })
        ));
    }

    #[test]
    fn skips_arrays_and_scalars_before_the_target() {
        let src = r#"{"a":[1,[2,{"b":"}"}]],"b":null,"c":true,"d":-1.5e3,"scripts":{"x":"y"}}"#;
        let m = parse(src, "scripts").unwrap();
        assert_eq!(names(&m), ["x"]);
    }
}
