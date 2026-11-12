use std::fs;
use std::path::PathBuf;

pub struct Preset {
    pub name: &'static str,
    pub ext: &'static str,
    pub cmd: &'static str,
    pub tpl: &'static str,
}

pub const PRESETS: [Preset; 5] = [
    Preset {
        name: "D",
        ext: "d",
        cmd: "dub run --single --quiet {file}",
        tpl: TPL_D,
    },
    Preset {
        name: "C",
        ext: "c",
        cmd: "gcc -O2 -o sol_bin {file} -lm && ./sol_bin",
        tpl: TPL_C,
    },
    Preset {
        name: "C++",
        ext: "cpp",
        cmd: "g++ -O2 -std=c++20 -o sol_bin {file} && ./sol_bin",
        tpl: TPL_CPP,
    },
    Preset {
        name: "Rust",
        ext: "rs",
        cmd: "rust-script {file}",
        tpl: TPL_RS,
    },
    Preset {
        name: "Python",
        ext: "py",
        cmd: "python3 {file}",
        tpl: TPL_PY,
    },
];

pub struct Config {
    pub root: PathBuf,
    pub lang: String,
    pub ext: String,
    pub cmd: String,
}

impl Config {
    pub fn load() -> Self {
        let root = std::env::var("AOC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join("aoc")
            });

        let mut cfg = Config {
            root,
            lang: PRESETS[0].name.into(),
            ext: PRESETS[0].ext.into(),
            cmd: PRESETS[0].cmd.into(),
        };

        if let Ok(s) = fs::read_to_string(cfg.config_file()) {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let v = v.trim().to_string();
                    match k.trim() {
                        "lang" => cfg.lang = v,
                        "ext" => cfg.ext = v,
                        "cmd" => cfg.cmd = v,
                        _ => {}
                    }
                }
            }
        }
        cfg
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(&self.root);
        let _ = fs::write(
            self.config_file(),
            format!("lang={}\next={}\ncmd={}\n", self.lang, self.ext, self.cmd),
        );
    }

    pub fn set_preset(&mut self, i: usize) {
        let p = &PRESETS[i];
        self.lang = p.name.into();
        self.ext = p.ext.into();
        self.cmd = p.cmd.into();
        self.save();
    }

    pub fn set_custom(&mut self, ext: String, mut cmd: String) {
        if !cmd.contains("{file}") {
            cmd.push_str(" {file}");
        }
        self.lang = "Others".into();
        self.ext = ext;
        self.cmd = cmd;
        self.save();
    }

    pub fn template(&self) -> &'static str {
        PRESETS
            .iter()
            .find(|p| p.name == self.lang)
            .map(|p| p.tpl)
            .unwrap_or("")
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
