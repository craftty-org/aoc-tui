use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn decode(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        o.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(j) = rest.find(';').filter(|&j| j <= 8) {
            let ent = &rest[1..j];
            let rep: Option<String> = match ent {
                "lt" => Some("<".into()),
                "gt" => Some(">".into()),
                "amp" => Some("&".into()),
                "quot" => Some("\"".into()),
                "apos" => Some("'".into()),
                "nbsp" => Some(" ".into()),
                e if e.starts_with('#') => {
                    let n = match e.strip_prefix("#x") {
                        Some(h) => u32::from_str_radix(h, 16).ok(),
                        None => e[1..].parse().ok(),
                    };
                    n.and_then(char::from_u32).map(|c| c.to_string())
                }
                _ => None,
            };
            if let Some(r) = rep {
                o.push_str(&r);
                rest = &rest[j + 1..];
                continue;
            }
        }
        o.push('&');
        rest = &rest[1..];
    }
    o.push_str(rest);
    o
}

fn wrap(segs: &[(String, Style)], width: usize) -> Vec<Line<'static>> {
    let width = width.max(10);
    let mut lines = vec![];
    let mut cur: Vec<Span<'static>> = vec![];
    let mut w = 0usize;
    for (t, st) in segs {
        for word in t.split_inclusive(' ') {
            let len = word.chars().count();
            let trimmed = word.trim_end().chars().count();
            if w > 0 && w + trimmed > width {
                lines.push(Line::from(std::mem::take(&mut cur)));
                w = 0;
            }
            if word.trim().is_empty() {
                let last_space = cur.last().map(|s| s.content.ends_with(' ')).unwrap_or(true);
                if w == 0 || last_space {
                    continue;
                }
            }
            cur.push(Span::styled(word.to_string(), *st));
            w += len;
        }
    }
    if !cur.is_empty() {
        lines.push(Line::from(cur));
    }
    lines
}

struct R {
    width: usize,
    out: Vec<Line<'static>>,
    segs: Vec<(String, Style)>,
    em: u32,
    code: u32,
    link: u32,
    pre: bool,
    heading: bool,
}

impl R {
    fn style(&self) -> Style {
        let mut s = Style::default();
        if self.heading {
            s = s.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        }
        if self.code > 0 {
            s = s.fg(Color::Green);
        }
        if self.link > 0 {
            s = s.fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
        }
        if self.em > 0 {
            s = s
                .fg(if self.code > 0 {
                    Color::White
                } else {
                    Color::Yellow
                })
                .add_modifier(Modifier::BOLD);
        }
        s
    }

    fn blank(&mut self) {
        if self.out.last().map(|l| !l.spans.is_empty()).unwrap_or(false) {
            self.out.push(Line::default());
        }
    }

    fn text(&mut self, t: &str) {
        let t = decode(t);
        let st = self.style();
        if self.pre {
            self.segs.push((t, st));
            return;
        }
        let mut s = String::new();
        let mut prev_ws = false;
        for c in t.chars() {
            if c.is_whitespace() {
                if !prev_ws {
                    s.push(' ');
                }
                prev_ws = true;
            } else {
                s.push(c);
                prev_ws = false;
            }
        }
        if s.trim().is_empty() && self.segs.is_empty() {
            return;
        }
        self.segs.push((s, st));
    }

    fn flush(&mut self) {
        if self.segs.is_empty() {
            return;
        }
        let segs = std::mem::take(&mut self.segs);
        if self.pre {
            let mut cur: Vec<Span<'static>> = vec![Span::raw("  ")];
            for (t, st) in segs {
                let mut parts = t.split('\n').peekable();
                while let Some(p) = parts.next() {
                    if !p.is_empty() {
                        cur.push(Span::styled(p.to_string(), st));
                    }
                    if parts.peek().is_some() {
                        self.out.push(Line::from(std::mem::replace(
                            &mut cur,
                            vec![Span::raw("  ")],
                        )));
                    }
                }
            }
            if cur.len() > 1 {
                self.out.push(Line::from(cur));
            }
        } else {
            let l = wrap(&segs, self.width);
            self.out.extend(l);
        }
    }

    fn tag(&mut self, raw: &str) {
        let closing = raw.starts_with('/');
        let name = raw
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let upd = |n: &mut u32| {
            if closing {
                *n = n.saturating_sub(1)
            } else {
                *n += 1
            }
        };
        match (name.as_str(), closing) {
            ("h2", false) => {
                self.flush();
                self.heading = true;
            }
            ("h2", true) => {
                self.flush();
                self.heading = false;
                self.blank();
            }
            ("p", false) | ("article", _) | ("br", _) | ("li", true) => self.flush(),
            ("p", true) | ("ul", true) | ("ol", true) => {
                self.flush();
                self.blank();
            }
            ("pre", false) => {
                self.flush();
                self.pre = true;
            }
            ("pre", true) => {
                self.flush();
                self.pre = false;
                self.blank();
            }
            ("li", false) => {
                self.flush();
                let st = self.style();
                self.segs.push(("• ".into(), st));
            }
            ("em", _) | ("strong", _) => upd(&mut self.em),
            ("code", _) => upd(&mut self.code),
            ("a", _) => upd(&mut self.link),
            _ => {}
        }
    }
}

pub fn render_html(html: &str, width: usize) -> Vec<Line<'static>> {
    let mut r = R {
        width,
        out: vec![],
        segs: vec![],
        em: 0,
        code: 0,
        link: 0,
        pre: false,
        heading: false,
    };
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        if i > 0 {
            r.text(&rest[..i]);
        }
        rest = &rest[i..];
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(j) => {
                    rest = &rest[j + 3..];
                    continue;
                }
                None => {
                    rest = "";
                    break;
                }
            }
        }
        match rest.find('>') {
            Some(j) => {
                r.tag(&rest[1..j]);
                rest = &rest[j + 1..];
            }
            None => {
                rest = "";
                break;
            }
        }
    }
    if !rest.is_empty() {
        r.text(rest);
    }
    r.flush();
    r.out
}

/// Puzzle HTML as plain text: no borders, paragraphs on single lines, ready to paste.
pub fn html_to_text(html: &str) -> String {
    render_html(html, 100_000)
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
