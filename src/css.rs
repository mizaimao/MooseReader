//! The part of CSS a terminal can show: bold, italic, underline, small caps,
//! alignment and first-line indent. Rules use the simple selectors ebook tools
//! write (`tag`, `.class`, `tag.class`); rules with combinators, pseudo-classes
//! or attribute selectors are skipped, as are @-rules such as @media.

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// The style an element ends up with, its parent's included.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Computed {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub small_caps: bool,
    pub align: Align,
    pub indent: bool,
}

/// What a rule sets; None keeps the inherited value.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Declared {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    small_caps: Option<bool>,
    align: Option<Align>,
    indent: Option<bool>,
}

impl Declared {
    fn merge(&mut self, later: &Declared) {
        self.bold = later.bold.or(self.bold);
        self.italic = later.italic.or(self.italic);
        self.underline = later.underline.or(self.underline);
        self.small_caps = later.small_caps.or(self.small_caps);
        self.align = later.align.or(self.align);
        self.indent = later.indent.or(self.indent);
    }

    fn apply(&self, parent: Computed) -> Computed {
        Computed {
            bold: self.bold.unwrap_or(parent.bold),
            italic: self.italic.unwrap_or(parent.italic),
            underline: self.underline.unwrap_or(parent.underline),
            small_caps: self.small_caps.unwrap_or(parent.small_caps),
            align: self.align.unwrap_or(parent.align),
            indent: self.indent.unwrap_or(parent.indent),
        }
    }
}

/// Cascade origin: the book's rules beat the reader's defaults whatever
/// their specificity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Origin {
    Reader,
    Book,
}

struct Rule {
    tag: Option<String>,
    classes: Vec<String>,
    origin: Origin,
    specificity: usize,
    order: usize,
    declared: Declared,
}

impl Rule {
    fn matches(&self, tag: &str, class: &str) -> bool {
        self.tag.as_deref().is_none_or(|t| t == tag)
            && (self.classes.iter())
                .all(|wanted| class.split_whitespace().any(|have| have == wanted))
    }
}

/// The reader's defaults, which a book's stylesheet can override.
const READER_CSS: &str = "
    b, strong, th { font-weight: bold }
    i, em, cite, dfn, var { font-style: italic }
    u, ins { text-decoration: underline }
    h1, h2, h3, h4, h5, h6 { font-weight: bold }
    h1, h2, h3, center { text-align: center }
";

pub struct Stylesheet {
    rules: Vec<Rule>,
    // Whether style="" attributes count; the plain look ignores them
    inline: bool,
}

impl Stylesheet {
    /// The reader's defaults alone.
    pub fn new() -> Self {
        let mut sheet = Stylesheet {
            rules: Vec::new(),
            inline: true,
        };
        sheet.add_rules(READER_CSS, Origin::Reader);
        sheet
    }

    /// The reader's defaults, ignoring everything the book says about style.
    pub fn plain() -> Self {
        Stylesheet {
            inline: false,
            ..Stylesheet::new()
        }
    }

    /// Adds a book's stylesheet after the ones already added.
    pub fn add(&mut self, css: &str) {
        self.add_rules(css, Origin::Book);
    }

    fn add_rules(&mut self, css: &str, origin: Origin) {
        let css = strip_comments(css);
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            // Statements such as @charset "UTF-8"; can sit in front of a selector
            let head = rest[..open].rsplit(';').next().unwrap_or("").trim();
            let Some(len) = block_len(&rest[open..]) else {
                break;
            };
            if !head.starts_with('@') {
                let declared = parse_declarations(&rest[open + 1..open + len - 1]);
                if declared != Declared::default() {
                    for selector in head.split(',') {
                        if let Some((tag, classes)) = parse_selector(selector.trim()) {
                            self.rules.push(Rule {
                                specificity: classes.len() * 10 + usize::from(tag.is_some()),
                                order: self.rules.len(),
                                tag,
                                classes,
                                origin,
                                declared,
                            });
                        }
                    }
                }
            }
            rest = &rest[open + len..];
        }
    }

    /// An element's style inside `parent`: matching rules in cascade order
    /// (origin, then specificity, then source order), then its style attribute.
    pub fn compute(&self, parent: &Computed, tag: &str, class: &str, inline: &str) -> Computed {
        let mut matching: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| r.matches(tag, class))
            .collect();
        matching.sort_by_key(|r| (r.origin, r.specificity, r.order));
        let mut declared = Declared::default();
        for rule in matching {
            declared.merge(&rule.declared);
        }
        if self.inline {
            declared.merge(&parse_declarations(inline));
        }
        declared.apply(*parent)
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start + 2..]
            .find("*/")
            .map_or("", |end| &rest[start + 2 + end + 2..]);
    }
    out.push_str(rest);
    out
}

/// Length of the `{…}` block that `s` starts with, nested blocks included.
fn block_len(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// `tag`, `.class`, `tag.class` or `.a.b`; any other selector gives None.
fn parse_selector(selector: &str) -> Option<(Option<String>, Vec<String>)> {
    let simple = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    if selector.is_empty() || !selector.chars().all(simple) {
        return None;
    }
    let mut parts = selector.split('.');
    let tag = parts.next()?;
    let classes: Vec<String> = parts.map(str::to_string).collect();
    if classes.iter().any(String::is_empty) {
        return None;
    }
    Some(((!tag.is_empty()).then(|| tag.to_ascii_lowercase()), classes))
}

fn parse_declarations(body: &str) -> Declared {
    let mut d = Declared::default();
    for declaration in body.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.to_ascii_lowercase().replace("!important", "");
        let value = value.trim();
        let words = || value.split_whitespace();
        match name.as_str() {
            "font-style" => {
                d.italic = Some(value.starts_with("italic") || value.starts_with("oblique"))
            }
            "font-weight" => d.bold = Some(is_bold(value)),
            "text-decoration" | "text-decoration-line" => {
                d.underline = Some(words().any(|w| w == "underline"))
            }
            "font-variant" | "font-variant-caps" => {
                d.small_caps = Some(words().any(|w| w == "small-caps" || w == "all-small-caps"))
            }
            "text-align" => {
                d.align = Some(match value {
                    "center" | "-webkit-center" => Align::Center,
                    "right" | "end" => Align::Right,
                    _ => Align::Left,
                })
            }
            "text-indent" => d.indent = Some(is_positive_length(value)),
            // The shorthand, as far as it names these properties
            "font" => {
                for word in words() {
                    match word {
                        "italic" | "oblique" => d.italic = Some(true),
                        "small-caps" => d.small_caps = Some(true),
                        w if is_bold(w) => d.bold = Some(true),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    d
}

fn is_bold(value: &str) -> bool {
    matches!(value, "bold" | "bolder") || value.parse::<u32>().is_ok_and(|w| w >= 600)
}

/// A first-line indent worth showing: a positive length. Zero, and the
/// negative hanging indents used for list-like paragraphs, are not.
fn is_positive_length(value: &str) -> bool {
    let number: String = value
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
        .collect();
    number.parse::<f64>().is_ok_and(|n| n > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(sheet: &Stylesheet, tag: &str, class: &str, inline: &str) -> Computed {
        sheet.compute(&Computed::default(), tag, class, inline)
    }

    #[test]
    fn reader_defaults_cover_the_html_tags() {
        let sheet = Stylesheet::new();
        assert!(style(&sheet, "b", "", "").bold);
        assert!(style(&sheet, "em", "", "").italic);
        assert_eq!(style(&sheet, "h2", "", "").align, Align::Center);
        assert_eq!(style(&sheet, "h5", "", "").align, Align::Left);
        assert_eq!(style(&sheet, "p", "", ""), Computed::default());
    }

    #[test]
    fn book_rules_by_class_and_tag() {
        let mut sheet = Stylesheet::new();
        sheet.add(
            r#"@charset "UTF-8";
            /* comment { with braces } */
            .calibre5 { font-style: italic }
            .calibre3, p.center { text-align: center; text-indent: 0 }
            .bold { font-weight: 700 !important }
            .tx { text-indent: 1.2em }
            .hang { text-indent: -1em }
            .sc { font-variant: small-caps }
            @media amzn-kf8 { .calibre5 { font-style: normal } }
            div p.x { font-weight: bold }
            a:hover { text-decoration: underline }
            i.plain { font-style: normal }"#,
        );
        assert!(style(&sheet, "span", "calibre5", "").italic);
        assert!(
            style(&sheet, "span", "calibre5 other", "").italic,
            "@media rules are skipped"
        );
        assert_eq!(style(&sheet, "div", "calibre3", "").align, Align::Center);
        assert_eq!(style(&sheet, "p", "center", "").align, Align::Center);
        assert_eq!(
            style(&sheet, "div", "center", "").align,
            Align::Left,
            "p.center needs a p"
        );
        assert!(style(&sheet, "span", "bold", "").bold);
        assert!(style(&sheet, "p", "tx", "").indent);
        assert!(!style(&sheet, "p", "hang", "").indent);
        assert!(style(&sheet, "span", "sc", "").small_caps);
        assert!(
            !style(&sheet, "p", "x", "").bold,
            "descendant selectors are skipped"
        );
        // The book's i.plain beats the reader's i, and inline style beats both
        assert!(!style(&sheet, "i", "plain", "").italic);
        assert!(style(&sheet, "i", "plain", "font-style: italic").italic);
        assert!(style(&sheet, "span", "", "font: italic bold 1em serif").bold);
    }

    #[test]
    fn styles_inherit_unless_overridden() {
        let mut sheet = Stylesheet::new();
        sheet.add(".it { font-style: italic } .c { text-align: center } .n { font-style: normal }");
        let outer = style(&sheet, "div", "it c", "");
        let inner = sheet.compute(&outer, "span", "", "");
        assert!(inner.italic && inner.align == Align::Center);
        assert!(!sheet.compute(&outer, "span", "n", "").italic);
    }

    #[test]
    fn weights_decorations_and_letter_case() {
        let mut sheet = Stylesheet::new();
        sheet.add(
            ".m { font-weight: 500 } .h { FONT-WEIGHT: 600 } .u { text-decoration: underline }
             .n { text-decoration: none } .i { Font-Style: Italic }",
        );
        assert!(!style(&sheet, "span", "m", "").bold);
        assert!(style(&sheet, "span", "h", "").bold);
        let underlined = style(&sheet, "span", "u", "");
        assert!(underlined.underline);
        assert!(!sheet.compute(&underlined, "span", "n", "").underline);
        assert!(style(&sheet, "span", "i", "").italic);
    }

    #[test]
    fn more_classes_beat_fewer_whatever_the_order() {
        let mut sheet = Stylesheet::new();
        sheet.add(".a.b { font-style: italic } .a { font-style: normal }");
        assert!(style(&sheet, "p", "a b", "").italic);
        assert!(!style(&sheet, "p", "a", "").italic);
        assert!(
            !style(&sheet, "p", "b", "").italic,
            ".a.b needs both classes"
        );
    }

    #[test]
    fn the_plain_sheet_ignores_style_attributes() {
        let sheet = Stylesheet::plain();
        assert!(!style(&sheet, "span", "", "font-style: italic").italic);
        assert!(style(&sheet, "i", "", "").italic);
    }

    #[test]
    fn broken_css_keeps_the_rules_before_it() {
        let mut sheet = Stylesheet::new();
        sheet.add(".ok { font-weight: bold } .broken { font-style: italic ");
        sheet.add("no braces at all");
        assert!(style(&sheet, "span", "ok", "").bold);
        assert!(!style(&sheet, "span", "broken", "").italic);
    }
}
