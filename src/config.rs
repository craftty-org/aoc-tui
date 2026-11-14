use std::fs;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Lang {
    pub name: String,
    pub ext: String,
    pub cmd: String,
}

pub struct Config {
    pub root: PathBuf,
    pub langs: Vec<Lang>,
    pub cur: usize,
    pub warn: Option<String>,
}

const DEFAULT_CONFIG: &str = r##"# aoc-tui languages
# One [Name] section per language; they appear in the L menu in this order.
#   ext = solution file extension (no dot)
#   cmd = shell command, run inside the day folder with input.txt on stdin.
#         {file} is the solution file name. Chain build + run with &&.
# Optional starter code: <data dir>/templates/template.<ext>

[D]
ext = d
cmd = dub run --single --quiet {file}

[C]
ext = c
cmd = gcc -O2 -o sol_bin {file} -lm && ./sol_bin

[C++]
ext = cpp
cmd = g++ -O2 -std=c++20 -o sol_bin {file} && ./sol_bin

[Rust]
ext = rs
cmd = rust-script {file}

[Python]
ext = py
cmd = python3 {file}

# [Go]
# ext = go
# cmd = go run {file}
"##;

/// Linux/macOS: $XDG_CONFIG_HOME/aoc or ~/.config/aoc. Windows: ~/aoc.
fn default_root() -> PathBuf {
    if cfg!(windows) {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".into());
        return PathBuf::from(home).join("aoc");
    }
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        if !x.trim().is_empty() {
            return PathBuf::from(x).join("aoc");
        }
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
        .join(".config")
        .join("aoc")
}

fn finish(l: Option<Lang>, langs: &mut Vec<Lang>, warns: &mut Vec<String>) {
    let Some(mut l) = l else { return };
    l.ext = l.ext.trim_start_matches('.').to_string();
    if l.ext.is_empty() || l.cmd.is_empty() {
        warns.push(format!("[{}] needs both ext and cmd", l.name));
        return;
    }
    if !l.cmd.contains("{file}") {
        l.cmd.push_str(" {file}");
    }
    langs.push(l);
}

fn parse(s: &str) -> (Vec<Lang>, Vec<String>) {
    let mut langs = vec![];
    let mut warns = vec![];
    let mut cur: Option<Lang> = None;
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            finish(cur.take(), &mut langs, &mut warns);
            cur = Some(Lang {
                name: name.trim().to_string(),
                ext: String::new(),
                cmd: String::new(),
            });
        } else if let (Some(l), Some((k, v))) = (cur.as_mut(), line.split_once('=')) {
            let v = v.trim().to_string();
            match k.trim() {
                "ext" => l.ext = v,
                "cmd" => l.cmd = v,
                _ => {}
            }
        }
    }
    finish(cur.take(), &mut langs, &mut warns);
    (langs, warns)
}

impl Config {
    pub fn load() -> Self {
        let root = std::env::var("AOC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_root());
        let mut cfg = Config {
            root,
            langs: vec![],
            cur: 0,
            warn: None,
        };
        cfg.reload();
        cfg
    }

    /// Re-reads the config file, keeping the current language if it still exists.
    pub fn reload(&mut self) {
        let keep = self
            .langs
            .get(self.cur)
            .map(|l| l.name.clone())
            .or_else(|| {
                fs::read_to_string(self.lang_file())
                    .ok()
                    .map(|s| s.trim().to_string())
            });
        let path = self.config_file();
        if !path.exists() {
            let _ = fs::create_dir_all(&self.root);
            let _ = fs::write(&path, DEFAULT_CONFIG);
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_CONFIG.into());
        let (mut langs, warns) = parse(&text);
        self.warn = None;
        if langs.is_empty() {
            langs = parse(DEFAULT_CONFIG).0;
            self.warn = Some(format!(
                "No valid [Language] sections in {} — using built-in defaults",
                path.display()
            ));
        } else if !warns.is_empty() {
            self.warn = Some(warns.join("; "));
        }
        self.cur = keep
            .and_then(|n| langs.iter().position(|l| l.name == n))
            .unwrap_or(0);
        self.langs = langs;
    }

    pub fn cur(&self) -> &Lang {
        &self.langs[self.cur]
    }

    pub fn select(&mut self, i: usize) {
        if i < self.langs.len() {
            self.cur = i;
            let _ = fs::create_dir_all(&self.root);
            let _ = fs::write(self.lang_file(), &self.langs[i].name);
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config")
    }

    fn lang_file(&self) -> PathBuf {
        self.root.join(".lang")
    }

    pub fn template(&self) -> String {
        let ext = &self.cur().ext;
        fs::read_to_string(self.root.join("templates").join(format!("template.{ext}")))
            .unwrap_or_else(|_| builtin_template(ext).to_string())
    }

    pub fn day_dir(&self, year: i32, day: u32) -> PathBuf {
        self.root.join(year.to_string()).join(format!("day{day:02}"))
    }

    pub fn session_file(&self) -> PathBuf {
        self.root.join(".session")
    }

    pub fn load_session(&self) -> String {
        if let Ok(s) = std::env::var("AOC_SESSION") {
            if !s.trim().is_empty() {
                return s.trim().to_string();
            }
        }
        fs::read_to_string(self.session_file())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
}

fn builtin_template(ext: &str) -> &'static str {
    match ext {
        "py" => TPL_PY,
        "rs" => TPL_RS,
        "d" => TPL_D,
        "c" => TPL_C,
        "cpp" => TPL_CPP,
        _ => "",
    }
}

const TPL_PY: &str = r#"import sys

data = sys.stdin.read()


def part1(data):
    return "TODO"


def part2(data):
    return "TODO"


print(part1(data))
print(part2(data))
"#;

const TPL_RS: &str = r#"#!/usr/bin/env rust-script
use std::io::Read;

fn part1(_input: &str) -> String {
    "TODO".into()
}

fn part2(_input: &str) -> String {
    "TODO".into()
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    println!("{}", part1(&input));
    println!("{}", part2(&input));
}
"#;

const TPL_D: &str = r#"/+ dub.sdl:
    name "solution"
+/
import std.stdio;
import std.string;
import std.conv;
import std.algorithm;
import std.array;
import std.range;

string part1(string input)
{
    return "TODO";
}

string part2(string input)
{
    return "TODO";
}

void main()
{
    string input = stdin.byLineCopy.array.join("\n");
    writeln(part1(input));
    writeln(part2(input));
}
"#;

const TPL_C: &str = r#"#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    size_t cap = 1 << 16, len = 0, n;
    char *input = malloc(cap);
    while ((n = fread(input + len, 1, cap - len - 1, stdin)) > 0) {
        len += n;
        if (cap - len < 4096) {
            cap *= 2;
            input = realloc(input, cap);
        }
    }
    input[len] = '\0';

    printf("%s\n", "TODO"); /* part 1 */
    printf("%s\n", "TODO"); /* part 2 */

    free(input);
    return 0;
}
"#;

const TPL_CPP: &str = r#"#include <bits/stdc++.h>
using namespace std;

string part1(const string& input) { return "TODO"; }
string part2(const string& input) { return "TODO"; }

int main() {
    ios::sync_with_stdio(false);
    string input((istreambuf_iterator<char>(cin)), istreambuf_iterator<char>());
    cout << part1(input) << "\n" << part2(input) << "\n";
}
"#;
