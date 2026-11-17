//! KeyValues-Parser/Writer (das Textformat von VMF, VMT usw.).
//!
//! Aufbau:
//! ```text
//! blockname
//! {
//!     "key" "value"
//!     unterblock { ... }
//! }
//! ```

#[derive(Debug, Clone, PartialEq)]
pub enum KvItem {
    Pair(String, String),
    Block(KvBlock),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct KvBlock {
    pub name: String,
    pub items: Vec<KvItem>,
}

impl KvBlock {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), items: Vec::new() }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.items.iter().find_map(|i| match i {
            KvItem::Pair(k, v) if k.eq_ignore_ascii_case(key) => Some(v.as_str()),
            _ => None,
        })
    }

    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().filter_map(|i| match i {
            KvItem::Pair(k, v) => Some((k.as_str(), v.as_str())),
            _ => None,
        })
    }

    pub fn blocks(&self) -> impl Iterator<Item = &KvBlock> {
        self.items.iter().filter_map(|i| match i {
            KvItem::Block(b) => Some(b),
            _ => None,
        })
    }

    pub fn push(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.items.push(KvItem::Pair(key.into(), value.into()));
    }

    pub fn push_block(&mut self, block: KvBlock) {
        self.items.push(KvItem::Block(block));
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

#[derive(Debug)]
pub struct KvError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for KvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Zeile {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for KvError {}

fn tokenize(src: &str) -> Vec<(Tok, usize)> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    let mut line = 1;
    while let Some(&c) = chars.peek() {
        match c {
            '\n' => {
                line += 1;
                chars.next();
            }
            c if c.is_whitespace() || c == '\u{feff}' => {
                chars.next();
            }
            '/' => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            line += 1;
                            break;
                        }
                    }
                } else {
                    // Einzelner Slash als Teil eines unquotierten Strings
                    let mut s = String::from("/");
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                            break;
                        }
                        s.push(c);
                        chars.next();
                    }
                    out.push((Tok::Str(s), line));
                }
            }
            '{' => {
                chars.next();
                out.push((Tok::Open, line));
            }
            '}' => {
                chars.next();
                out.push((Tok::Close, line));
            }
            '"' => {
                chars.next();
                let start_line = line;
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\n' => {
                            line += 1;
                            s.push(c);
                        }
                        _ => s.push(c),
                    }
                }
                out.push((Tok::Str(s), start_line));
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                out.push((Tok::Str(s), line));
            }
        }
    }
    out
}

/// Parst eine komplette Datei und liefert die Top-Level-Blöcke.
pub fn parse(src: &str) -> Result<Vec<KvBlock>, KvError> {
    let toks = tokenize(src);
    let mut pos = 0;
    let mut root = Vec::new();
    while pos < toks.len() {
        let (tok, line) = &toks[pos];
        match tok {
            Tok::Str(name) => {
                pos += 1;
                match toks.get(pos) {
                    Some((Tok::Open, _)) => {
                        pos += 1;
                        let block = parse_block_body(name.clone(), &toks, &mut pos)?;
                        root.push(block);
                    }
                    // Top-Level-Paare (kommt in VMT vor: "LightmappedGeneric" { ... } ist Block,
                    // einzelne Paare werden ignoriert)
                    Some((Tok::Str(_), _)) => {
                        pos += 1;
                    }
                    _ => {
                        return Err(KvError { line: *line, msg: format!("'{{' nach '{name}' erwartet") })
                    }
                }
            }
            _ => return Err(KvError { line: *line, msg: "Unerwartetes Zeichen auf oberster Ebene".into() }),
        }
    }
    Ok(root)
}

fn parse_block_body(name: String, toks: &[(Tok, usize)], pos: &mut usize) -> Result<KvBlock, KvError> {
    let mut block = KvBlock::new(name);
    loop {
        let Some((tok, line)) = toks.get(*pos) else {
            return Err(KvError { line: toks.last().map(|t| t.1).unwrap_or(0), msg: format!("Block '{}' nicht geschlossen", block.name) });
        };
        match tok {
            Tok::Close => {
                *pos += 1;
                return Ok(block);
            }
            Tok::Open => return Err(KvError { line: *line, msg: "Unerwartetes '{'".into() }),
            Tok::Str(key) => {
                *pos += 1;
                match toks.get(*pos) {
                    Some((Tok::Str(val), _)) => {
                        block.items.push(KvItem::Pair(key.clone(), val.clone()));
                        *pos += 1;
                    }
                    Some((Tok::Open, _)) => {
                        *pos += 1;
                        let child = parse_block_body(key.clone(), toks, pos)?;
                        block.items.push(KvItem::Block(child));
                    }
                    _ => return Err(KvError { line: *line, msg: format!("Wert für '{key}' fehlt") }),
                }
            }
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('"', "'")
}

pub fn write_block(out: &mut String, block: &KvBlock, depth: usize) {
    let indent = "\t".repeat(depth);
    out.push_str(&indent);
    out.push_str(&block.name);
    out.push('\n');
    out.push_str(&indent);
    out.push_str("{\n");
    for item in &block.items {
        match item {
            KvItem::Pair(k, v) => {
                out.push_str(&indent);
                out.push('\t');
                out.push('"');
                out.push_str(&escape(k));
                out.push_str("\" \"");
                out.push_str(&escape(v));
                out.push_str("\"\n");
            }
            KvItem::Block(b) => write_block(out, b, depth + 1),
        }
    }
    out.push_str(&indent);
    out.push_str("}\n");
}

pub fn write(blocks: &[KvBlock]) -> String {
    let mut out = String::new();
    for b in blocks {
        write_block(&mut out, b, 0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let src = r#"
// Kommentar
versioninfo
{
	"editorversion" "400"
}
world
{
	"id" "1"
	solid
	{
		"id" "2"
		side { "id" "3" "plane" "(0 0 0) (1 0 0) (1 1 0)" }
	}
}
"#;
        let blocks = parse(src).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].get("editorversion"), Some("400"));
        let solid = blocks[1].blocks().next().unwrap();
        assert_eq!(solid.name, "solid");
        let side = solid.blocks().next().unwrap();
        assert_eq!(side.get("plane"), Some("(0 0 0) (1 0 0) (1 1 0)"));
        let text = write(&blocks);
        assert_eq!(parse(&text).unwrap(), blocks);
    }
}
