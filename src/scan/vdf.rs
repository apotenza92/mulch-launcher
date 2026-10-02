//! Minimal parser for Valve's KeyValues text format (`.vdf` / `.acf`).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Obj(Vdf),
}

#[derive(Debug, Clone, Default)]
pub struct Vdf(pub HashMap<String, Value>);

impl Vdf {
    /// Case-insensitive key lookup (Valve files are inconsistent about case).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }

    pub fn obj(&self, key: &str) -> Option<&Vdf> {
        match self.get(key)? {
            Value::Obj(o) => Some(o),
            Value::Str(_) => None,
        }
    }

    pub fn objects(&self) -> impl Iterator<Item = (&String, &Vdf)> {
        self.0.iter().filter_map(|(k, v)| match v {
            Value::Obj(o) => Some((k, o)),
            Value::Str(_) => None,
        })
    }
}

enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                s.push(match escaped {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        '"' => break,
                        other => s.push(other),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    tokens
}

/// Parses a whole file. The top level usually holds a single named object,
/// e.g. `"AppState" { ... }`.
pub fn parse(text: &str) -> Vdf {
    let tokens = tokenize(text);
    let mut pos = 0;
    parse_object(&tokens, &mut pos)
}

fn parse_object(tokens: &[Token], pos: &mut usize) -> Vdf {
    let mut map = HashMap::new();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Close => {
                *pos += 1;
                break;
            }
            Token::Open => {
                // Stray brace; skip it.
                *pos += 1;
            }
            Token::Str(key) => {
                let key = key.clone();
                *pos += 1;
                match tokens.get(*pos) {
                    Some(Token::Str(value)) => {
                        map.insert(key, Value::Str(value.clone()));
                        *pos += 1;
                    }
                    Some(Token::Open) => {
                        *pos += 1;
                        map.insert(key, Value::Obj(parse_object(tokens, pos)));
                    }
                    _ => break,
                }
            }
        }
    }
    Vdf(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"730"		"73992020124"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        let root = parse(text);
        let folders = root.obj("libraryfolders").unwrap();
        assert_eq!(folders.obj("0").unwrap().str("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(folders.obj("1").unwrap().str("PATH"), Some(r"D:\SteamLibrary"));
        assert!(folders.obj("0").unwrap().obj("apps").unwrap().str("730").is_some());
    }
}
