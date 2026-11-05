//! JSON, as far as pandoc's output needs it: a small parser, so that reading a .docx costs the crate no dependency.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn at(&self, i: usize) -> Option<&Value> {
        match self {
            Value::Arr(a) => a.get(i),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> &[Value] {
        match self {
            Value::Arr(a) => a,
            _ => &[],
        }
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { b: text.as_bytes(), i: 0, text };
    let v = p.value(0)?;
    p.space();
    if p.i != p.b.len() {
        return Err(format!("JSON: unexpected text at {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    text: &'a str,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.space();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("JSON: expected '{}' at {}", c as char, self.i))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > 500 {
            return Err("JSON: nested too deep".into());
        }
        self.space();
        match self.b.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                let mut o = Vec::new();
                self.space();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Value::Obj(o));
                }
                loop {
                    self.space();
                    let k = self.string()?;
                    self.eat(b':')?;
                    o.push((k, self.value(depth + 1)?));
                    self.space();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Value::Obj(o));
                        }
                        _ => return Err(format!("JSON: expected ',' or '}}' at {}", self.i)),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.space();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Value::Arr(a));
                }
                loop {
                    a.push(self.value(depth + 1)?);
                    self.space();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Value::Arr(a));
                        }
                        _ => return Err(format!("JSON: expected ',' or ']' at {}", self.i)),
                    }
                }
            }
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') if self.text[self.i..].starts_with("true") => {
                self.i += 4;
                Ok(Value::Bool(true))
            }
            Some(b'f') if self.text[self.i..].starts_with("false") => {
                self.i += 5;
                Ok(Value::Bool(false))
            }
            Some(b'n') if self.text[self.i..].starts_with("null") => {
                self.i += 4;
                Ok(Value::Null)
            }
            Some(c) if c.is_ascii_digit() || *c == b'-' => {
                let start = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                    self.i += 1;
                }
                self.text[start..self.i].parse().map(Value::Num).map_err(|_| format!("JSON: bad number at {start}"))
            }
            _ => Err(format!("JSON: unexpected text at {}", self.i)),
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self.text.get(self.i..self.i + 4).ok_or("JSON: bad \\u escape")?;
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| "JSON: bad \\u escape".to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        if self.b.get(self.i) != Some(&b'"') {
            return Err(format!("JSON: expected a string at {}", self.i));
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.i..];
            let n = rest.find(['"', '\\']).ok_or("JSON: unterminated string")?;
            out.push_str(&rest[..n]);
            self.i += n;
            if self.b[self.i] == b'"' {
                self.i += 1;
                return Ok(out);
            }
            let esc = *self.b.get(self.i + 1).ok_or("JSON: unterminated string")?;
            self.i += 2;
            match esc {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'u' => {
                    let mut c = self.hex4()?;
                    if (0xD800..0xDC00).contains(&c) && self.text[self.i..].starts_with("\\u") {
                        self.i += 2;
                        let low = self.hex4()?;
                        c = 0x10000 + ((c - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
                    }
                    out.push(char::from_u32(c).unwrap_or('\u{fffd}'));
                }
                other => out.push(other as char),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_pandoc_writes() {
        let v = parse(r#"{"t":"Str","c":"café \"x\" 𝒳\n","n":[1,-2.5e1,true,null,[]],"o":{}}"#).unwrap();
        assert_eq!(v.get("t").and_then(Value::as_str), Some("Str"));
        assert_eq!(v.get("c").and_then(Value::as_str), Some("café \"x\" 𝒳\n"));
        assert_eq!(v.get("n").unwrap().as_array().len(), 5);
        assert_eq!(v.get("n").unwrap().at(1), Some(&Value::Num(-25.0)));
        assert!(parse("[1,").is_err());
        assert!(parse("{} x").is_err());
    }
}
