use std::collections::HashMap;

use regex::Regex;

pub const UA: &str = "aoc-tui (rust) personal use";

fn err_str(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(c) => format!("HTTP {c}"),
        e => e.to_string(),
    }
}

pub fn http_get(url: &str, session: &str) -> Result<String, String> {
    let mut req = ureq::get(url).header("User-Agent", UA);
    if !session.is_empty() {
        req = req.header("Cookie", format!("session={session}"));
    }
    let mut resp = req.call().map_err(err_str)?;
    resp.body_mut().read_to_string().map_err(err_str)
}

pub fn http_submit(
    year: i32,
    day: u32,
    level: u8,
    answer: &str,
    session: &str,
) -> Result<String, String> {
    let url = format!("https://adventofcode.com/{year}/day/{day}/answer");
    let lvl = level.to_string();
    let mut resp = ureq::post(&url)
        .header("User-Agent", UA)
        .header("Cookie", format!("session={session}"))
        .send_form([("level", lvl.as_str()), ("answer", answer)])
        .map_err(err_str)?;
    let body = resp.body_mut().read_to_string().map_err(err_str)?;
    let main = Regex::new(r"(?s)<main>(.*?)</main>")
        .unwrap()
        .captures(&body)
        .map(|c| c[1].to_string())
        .unwrap_or(body);
    let text = Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&main, " ")
        .to_string();
    let text = crate::render::decode(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(match text.find("[Return") {
        Some(i) => text[..i].trim().to_string(),
        None => text,
    })
}

pub fn parse_stars(html: &str) -> HashMap<u32, u8> {
    let re = Regex::new(r#"aria-label="Day (\d+)(?:, (one|two) stars?)?""#).unwrap();
    let mut m = HashMap::new();
    for c in re.captures_iter(html) {
        let d: u32 = c[1].parse().unwrap_or(0);
        let s = match c.get(2).map(|x| x.as_str()) {
            Some("one") => 1,
            Some("two") => 2,
            _ => 0,
        };
        m.insert(d, s);
    }
    m
}

pub fn extract(html: &str) -> String {
    let re = Regex::new(r#"(?s)<article class="day-desc">(.*?)</article>"#).unwrap();
    let parts: Vec<String> = re.captures_iter(html).map(|c| c[1].to_string()).collect();
    if parts.is_empty() {
        "<p>(no puzzle text found on that page)</p>".into()
    } else {
        parts.join("\n")
    }
}
