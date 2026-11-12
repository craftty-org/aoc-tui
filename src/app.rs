use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, TimeZone};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;

use crate::clipboard;
use crate::config::{Config, PRESETS};
use crate::http::{self, extract, parse_stars};
use crate::render;

#[derive(PartialEq, Clone, Copy)]
pub enum Pane {
    Years,
    Days,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Puzzle = 0,
    Input = 1,
    Output = 2,
}

fn tab_from(i: usize) -> Tab {
    match i % 3 {
        0 => Tab::Puzzle,
        1 => Tab::Input,
        _ => Tab::Output,
    }
}

#[derive(Clone)]
pub enum Prompt {
    Session,
    Submit(u8),
    CustomExt,
    CustomCmd(String),
}

pub enum Mode {
    Normal,
    Prompt { kind: Prompt, buf: String },
    Popup(String),
    Lang(usize),
}

pub struct View {
    pub year: i32,
    pub day: u32,
    pub desc: Option<String>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub answers: Vec<String>,
    pub tab: Tab,
    pub scroll: [usize; 3],
}

pub struct RunResult {
    pub cmd: String,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

impl RunResult {
    pub fn render(&self) -> String {
        format!(
            "$ {}\nexit: {}   time: {:.2?}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.cmd,
            self.code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into()),
            self.elapsed,
            self.stdout.trim_end(),
            self.stderr.trim_end()
        )
    }
}

pub enum Msg {
    Stars(i32, Result<HashMap<u32, u8>, String>),
    Puzzle(i32, u32, Result<String, String>),
    Input(i32, u32, Result<String, String>),
    Run(Result<RunResult, String>),
    Submit(i32, u32, Result<String, String>),
}

pub fn days_in(year: i32) -> u32 {
    if year >= 2025 {
        12
    } else {
        25
    }
}

/// "It opens on December 1 at midnight, in 31 days."
pub fn opens_text(year: i32) -> String {
    let Some(t) = Local.with_ymd_and_hms(year, 12, 1, 0, 0, 0).earliest() else {
        return "It opens on December 1 at midnight.".into();
    };
    let days = (t - Local::now()).num_days().max(0);
    format!("It opens on December 1 at midnight, in {days} days.")
}

pub struct App {
    pub cfg: Config,
    pub session: String,
    pub tx: Sender<Msg>,
    pub now: (i32, u32, u32),
    pub years: Vec<i32>,
    pub year_state: ListState,
    pub day_state: ListState,
    pub pane: Pane,
    pub stars: HashMap<i32, HashMap<u32, u8>>,
    pub stars_req: HashSet<i32>,
    pub view: Option<View>,
    pub mode: Mode,
    pub status: String,
    pub edit: Option<PathBuf>,
    pub running: bool,
    pub quit: bool,
    pub page: usize,
}

impl App {
    pub fn new(tx: Sender<Msg>) -> Self {
        let cfg = Config::load();
        let today = Local::now().date_naive();
        let now = (today.year(), today.month(), today.day());
        let years: Vec<i32> = (2015..=now.0).rev().collect();
        let in_season = now.1 == 12;
        let mut year_state = ListState::default();
        // Land on the newest year that has actually started.
        year_state.select(Some(if in_season {
            0
        } else {
            1.min(years.len() - 1)
        }));
        let mut day_state = ListState::default();
        let d = if in_season {
            now.2.min(days_in(now.0))
        } else {
            1
        };
        day_state.select(Some(d as usize - 1));
        let session = cfg.load_session();
        let status = if session.is_empty() {
            "No session cookie — press S to paste yours (or export AOC_SESSION)".into()
        } else {
            "Ready".into()
        };
        let mut app = App {
            cfg,
            session,
            tx,
            now,
            years,
            year_state,
            day_state,
            pane: Pane::Days,
            stars: HashMap::new(),
            stars_req: HashSet::new(),
            view: None,
            mode: Mode::Normal,
            status,
            edit: None,
            running: false,
            quit: false,
            page: 10,
        };
        app.ensure_stars();
        app
    }

    pub fn year(&self) -> i32 {
        self.years[self.year_state.selected().unwrap_or(0)]
    }

    pub fn day(&self) -> u32 {
        self.day_state.selected().unwrap_or(0) as u32 + 1
    }

    pub fn started(&self, y: i32) -> bool {
        y < self.now.0 || (y == self.now.0 && self.now.1 == 12)
    }

    pub fn unlocked(&self, y: i32, d: u32) -> bool {
        let (cy, cm, cd) = self.now;
        y < cy || (y == cy && cm == 12 && d <= cd)
    }

    // ── background work ──
    fn ensure_stars(&mut self) {
        let y = self.year();
        self.fetch_stars(y);
    }

    fn fetch_stars(&mut self, y: i32) {
        if self.session.is_empty() || !self.started(y) || !self.stars_req.insert(y) {
            return;
        }
        let (tx, s) = (self.tx.clone(), self.session.clone());
        thread::spawn(move || {
            let r = http::http_get(&format!("https://adventofcode.com/{y}"), &s)
                .map(|h| parse_stars(&h));
            let _ = tx.send(Msg::Stars(y, r));
        });
    }

    fn fetch_puzzle(&self, y: i32, d: u32) {
        let (tx, s) = (self.tx.clone(), self.session.clone());
        thread::spawn(move || {
            let r = http::http_get(&format!("https://adventofcode.com/{y}/day/{d}"), &s);
            let _ = tx.send(Msg::Puzzle(y, d, r));
        });
    }

    fn fetch_input(&mut self, y: i32, d: u32) {
        if self.session.is_empty() {
            self.status = "Need a session cookie to download your input — press S".into();
            return;
        }
        let (tx, s) = (self.tx.clone(), self.session.clone());
        thread::spawn(move || {
            let r = http::http_get(&format!("https://adventofcode.com/{y}/day/{d}/input"), &s);
            let _ = tx.send(Msg::Input(y, d, r));
        });
    }

    pub fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Stars(y, r) => match r {
                Ok(map) => {
                    self.stars.insert(y, map);
                }
                Err(e) => {
                    self.stars_req.remove(&y);
                    self.status = format!("Couldn't load stars for {y}: {e}");
                }
            },
            Msg::Puzzle(y, d, r) => match r {
                Ok(html) => {
                    let dir = self.cfg.day_dir(y, d);
                    let _ = fs::create_dir_all(&dir);
                    let _ = fs::write(dir.join("puzzle.html"), &html);
                    if let Some(v) = &mut self.view {
                        if v.year == y && v.day == d {
                            v.desc = Some(extract(&html));
                        }
                    }
                    self.status = format!("Puzzle {y} day {d} loaded");
                }
                Err(e) => self.status = format!("Puzzle download failed: {e}"),
            },
            Msg::Input(y, d, r) => match r {
                Ok(text) => {
                    let dir = self.cfg.day_dir(y, d);
                    let _ = fs::create_dir_all(&dir);
                    let _ = fs::write(dir.join("input.txt"), &text);
                    if let Some(v) = &mut self.view {
                        if v.year == y && v.day == d {
                            v.input = Some(text);
                        }
                    }
                    self.status = format!("Input {y} day {d} downloaded");
                }
                Err(e) => self.status = format!("Input download failed: {e}"),
            },
            Msg::Run(r) => {
                self.running = false;
                match r {
                    Ok(rr) => {
                        if let Some(v) = &mut self.view {
                            v.answers = rr
                                .stdout
                                .lines()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .take(2)
                                .collect();
                            v.output = Some(rr.render());
                            v.tab = Tab::Output;
                            v.scroll[2] = 0;
                        }
                        self.status = format!(
                            "Finished in {:.2?} — press 1 / 2 to submit part 1 / 2",
                            rr.elapsed
                        );
                    }
                    Err(e) => self.status = format!("Run failed: {e}"),
                }
            }
            Msg::Submit(y, d, r) => match r {
                Ok(text) => {
                    let ok = text.contains("the right answer") && !text.contains("not the right");
                    self.mode = Mode::Popup(text);
                    if ok {
                        self.stars_req.remove(&y);
                        self.fetch_stars(y);
                        self.fetch_puzzle(y, d);
                    }
                }
                Err(e) => self.status = format!("Submit failed: {e}"),
            },
        }
    }

    // ── navigation ──
    fn open_day(&mut self, y: i32, d: u32) {
        if !self.started(y) {
            self.status = format!(
                "Advent of Code {y} hasn't started yet. {}",
                opens_text(y)
            );
            return;
        }
        if !self.unlocked(y, d) {
            self.status = format!("{y} day {d} isn't unlocked yet");
            return;
        }
        let dir = self.cfg.day_dir(y, d);
        let desc = fs::read_to_string(dir.join("puzzle.html"))
            .ok()
            .map(|h| extract(&h));
        let input = fs::read_to_string(dir.join("input.txt")).ok();
        let need_puzzle = desc.is_none();
        let need_input = input.is_none();
        self.view = Some(View {
            year: y,
            day: d,
            desc,
            input,
            output: None,
            answers: vec![],
            tab: Tab::Puzzle,
            scroll: [0; 3],
        });
        if need_puzzle {
            self.status = "Downloading puzzle…".into();
            self.fetch_puzzle(y, d);
        } else {
            self.status = format!("{y} day {d} (cached — press R to refresh)");
        }
        if need_input {
            self.fetch_input(y, d);
        }
    }

    fn move_sel(&mut self, delta: i32) {
        match self.pane {
            Pane::Years => {
                let n = self.years.len() as i32;
                let i = self.year_state.selected().unwrap_or(0) as i32;
                self.year_state
                    .select(Some((i + delta).clamp(0, n - 1) as usize));
                let nd = days_in(self.year());
                if self.day() > nd {
                    self.day_state.select(Some(nd as usize - 1));
                }
                self.ensure_stars();
            }
            Pane::Days => {
                let n = days_in(self.year()) as i32;
                let i = self.day_state.selected().unwrap_or(0) as i32;
                self.day_state
                    .select(Some((i + delta).clamp(0, n - 1) as usize));
            }
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if matches!(self.mode, Mode::Popup(_)) {
            self.mode = Mode::Normal;
            return;
        }
        if let Mode::Lang(i) = &mut self.mode {
            match k.code {
                KeyCode::Up | KeyCode::Char('k') => *i = i.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => *i = (*i + 1).min(PRESETS.len()),
                KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
                KeyCode::Enter => {
                    let sel = *i;
                    self.mode = Mode::Normal;
                    self.choose_lang(sel);
                }
                KeyCode::Char(c @ '1'..='6') => {
                    self.mode = Mode::Normal;
                    self.choose_lang(c as usize - '1' as usize);
                }
                _ => {}
            }
            return;
        }
        if let Mode::Prompt { kind, buf } = &mut self.mode {
            match k.code {
                KeyCode::Char(c) => buf.push(c),
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Esc => self.mode = Mode::Normal,
                KeyCode::Enter => {
                    let b = buf.clone();
                    let kd = kind.clone();
                    self.mode = Mode::Normal;
                    self.finish_prompt(kd, b);
                }
                _ => {}
            }
            return;
        }
        if self.view.is_some() {
            self.puzzle_key(k);
        } else {
            self.picker_key(k);
        }
    }

    fn picker_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('L') => self.open_lang(),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::PageUp => self.move_sel(-5),
            KeyCode::PageDown => self.move_sel(5),
            KeyCode::Left | KeyCode::Char('h') => self.pane = Pane::Years,
            KeyCode::Right | KeyCode::Char('l') => self.pane = Pane::Days,
            KeyCode::Tab => {
                self.pane = if self.pane == Pane::Years {
                    Pane::Days
                } else {
                    Pane::Years
                }
            }
            KeyCode::Enter => match self.pane {
                Pane::Years => self.pane = Pane::Days,
                Pane::Days => {
                    let (y, d) = (self.year(), self.day());
                    self.open_day(y, d);
                }
            },
            KeyCode::Char('S') => {
                self.mode = Mode::Prompt {
                    kind: Prompt::Session,
                    buf: String::new(),
                }
            }
            _ => {}
        }
    }

    fn scroll(&mut self, delta: i64) {
        if let Some(v) = &mut self.view {
            let t = v.tab as usize;
            v.scroll[t] = if delta < 0 {
                v.scroll[t].saturating_sub((-delta) as usize)
            } else {
                v.scroll[t].saturating_add(delta as usize)
            };
        }
    }

    fn set_tab(&mut self, t: Tab) {
        if let Some(v) = &mut self.view {
            v.tab = t;
        }
    }

    fn cycle_tab(&mut self, d: i32) {
        if let Some(v) = &mut self.view {
            v.tab = tab_from((v.tab as i32 + d + 3) as usize);
        }
    }

    fn puzzle_key(&mut self, k: KeyEvent) {
        let page = self.page.max(1) as i64;
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char('d') if ctrl => self.scroll(page / 2),
            KeyCode::Char('u') if ctrl => self.scroll(-page / 2),
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => self.view = None,
            KeyCode::Char('p') => self.set_tab(Tab::Puzzle),
            KeyCode::Char('i') => self.set_tab(Tab::Input),
            KeyCode::Char('o') => self.set_tab(Tab::Output),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => self.cycle_tab(1),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => self.cycle_tab(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll(page),
            KeyCode::PageUp => self.scroll(-page),
            KeyCode::Home | KeyCode::Char('g') => self.scroll(i64::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.scroll(i64::MAX / 2),
            KeyCode::Char('e') => self.start_edit(),
            KeyCode::Char('y') => self.copy_tab(),
            KeyCode::Char('L') => self.open_lang(),
            KeyCode::Char('r') => self.start_run(),
            KeyCode::Char('1') => self.start_submit(1),
            KeyCode::Char('2') => self.start_submit(2),
            KeyCode::Char('S') => {
                self.mode = Mode::Prompt {
                    kind: Prompt::Session,
                    buf: String::new(),
                }
            }
            KeyCode::Char('R') => {
                if let Some(v) = &self.view {
                    let (y, d) = (v.year, v.day);
                    self.status = "Refreshing puzzle…".into();
                    self.fetch_puzzle(y, d);
                    self.stars_req.remove(&y);
                    self.fetch_stars(y);
                }
            }
            KeyCode::Char(']') => {
                if let Some(v) = &self.view {
                    let (y, d) = (v.year, v.day);
                    if d < days_in(y) {
                        self.open_day(y, d + 1);
                    }
                }
            }
            KeyCode::Char('[') => {
                if let Some(v) = &self.view {
                    let (y, d) = (v.year, v.day);
                    if d > 1 {
                        self.open_day(y, d - 1);
                    }
                }
            }
            _ => {}
        }
    }

    /// Copies whatever tab is showing (puzzle text, input or output) to the clipboard.
    fn copy_tab(&mut self) {
        let picked = self.view.as_ref().and_then(|v| match v.tab {
            Tab::Puzzle => v.desc.as_ref().map(|d| (render::html_to_text(d), "puzzle")),
            Tab::Input => v.input.clone().map(|s| (s, "input")),
            Tab::Output => v.output.clone().map(|s| (s, "output")),
        });
        let Some((text, what)) = picked else {
            self.status = "Nothing to copy on this tab yet".into();
            return;
        };
        self.status = match clipboard::copy_to_clipboard(&text) {
            Ok(how) => format!("Copied {what} to clipboard ({how})"),
            Err(e) => format!("Couldn't copy: {e}"),
        };
    }

    fn open_lang(&mut self) {
        let i = PRESETS
            .iter()
            .position(|p| p.name == self.cfg.lang)
            .unwrap_or(PRESETS.len());
        self.mode = Mode::Lang(i);
    }

    fn choose_lang(&mut self, i: usize) {
        if i < PRESETS.len() {
            self.cfg.set_preset(i);
            self.status = format!(
                "Language set to {} (saved) — solutions use .{}",
                self.cfg.lang, self.cfg.ext
            );
        } else {
            self.mode = Mode::Prompt {
                kind: Prompt::CustomExt,
                buf: String::new(),
            };
        }
    }

    fn start_edit(&mut self) {
        let Some(v) = &self.view else { return };
        let dir = self.cfg.day_dir(v.year, v.day);
        let _ = fs::create_dir_all(&dir);
        let p = dir.join(format!("solution.{}", self.cfg.ext));
        if !p.exists() {
            let _ = fs::write(&p, self.cfg.template());
        }
        self.edit = Some(p);
    }

    fn start_run(&mut self) {
        if self.running {
            return;
        }
        let Some(v) = &self.view else { return };
        let dir = self.cfg.day_dir(v.year, v.day);
        let sol = format!("solution.{}", self.cfg.ext);
        let input = dir.join("input.txt");
        if !dir.join(&sol).exists() {
            self.status = "No solution file yet — press e to create and edit one".into();
            return;
        }
        if self.cfg.cmd.is_empty() {
            self.status = "No run command set — press L to choose a language".into();
            return;
        }
        if !input.exists() {
            self.status = "Input hasn't been downloaded yet".into();
            return;
        }
        self.running = true;
        self.status = "Running…".into();
        let tx = self.tx.clone();
        let cmd = self.cfg.cmd.replace("{file}", &sol);
        thread::spawn(move || {
            let t = Instant::now();
            let res = (|| -> Result<RunResult, String> {
                let f = fs::File::open(&input).map_err(|e| e.to_string())?;
                let out = Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .current_dir(&dir)
                    .stdin(Stdio::from(f))
                    .output()
                    .map_err(|e| e.to_string())?;
                Ok(RunResult {
                    cmd: cmd.clone(),
                    code: out.status.code(),
                    stdout: String::from_utf8_lossy(&out.stdout).into(),
                    stderr: String::from_utf8_lossy(&out.stderr).into(),
                    elapsed: t.elapsed(),
                })
            })();
            let _ = tx.send(Msg::Run(res));
        });
    }

    fn start_submit(&mut self, level: u8) {
        let Some(v) = &self.view else { return };
        if self.session.is_empty() {
            self.status = "Need a session cookie to submit — press S".into();
            return;
        }
        let buf = v
            .answers
            .get(level as usize - 1)
            .cloned()
            .unwrap_or_default();
        self.mode = Mode::Prompt {
            kind: Prompt::Submit(level),
            buf,
        };
    }

    fn finish_prompt(&mut self, kind: Prompt, buf: String) {
        let buf = buf.trim().to_string();
        match kind {
            Prompt::Session => {
                if buf.is_empty() {
                    return;
                }
                let _ = fs::create_dir_all(&self.cfg.root);
                let _ = fs::write(self.cfg.session_file(), &buf);
                self.session = buf;
                self.stars_req.clear();
                self.ensure_stars();
                self.status = "Session saved".into();
            }
            Prompt::CustomExt => {
                let ext = buf.trim_start_matches('.').to_string();
                if ext.is_empty() {
                    return;
                }
                self.mode = Mode::Prompt {
                    kind: Prompt::CustomCmd(ext),
                    buf: String::new(),
                };
            }
            Prompt::CustomCmd(ext) => {
                if buf.is_empty() {
                    return;
                }
                self.cfg.set_custom(ext, buf);
                self.status = format!("Language set to Others (.{}) — saved", self.cfg.ext);
            }
            Prompt::Submit(level) => {
                if buf.is_empty() {
                    return;
                }
                let Some(v) = &self.view else { return };
                let (y, d) = (v.year, v.day);
                let (tx, s) = (self.tx.clone(), self.session.clone());
                self.status = format!("Submitting part {level}…");
                thread::spawn(move || {
                    let r = http::http_submit(y, d, level, &buf, &s);
                    let _ = tx.send(Msg::Submit(y, d, r));
                });
            }
        }
    }
}
