mod app;
mod clipboard;
mod config;
mod http;
mod render;
mod ui;

use std::io;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event};

fn run(term: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut app = app::App::new(tx);

    loop {
        while let Ok(m) = rx.try_recv() {
            app.on_msg(m);
        }

        term.draw(|f| ui::ui(f, &mut app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                app.on_key(k);
            }
        }

        if let Some(path) = app.edit.take() {
            ratatui::restore();
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "vi".into());
            let mut parts = editor.split_whitespace();
            let prog = parts.next().unwrap_or("vi").to_string();
            let args: Vec<&str> = parts.collect();
            let st = Command::new(&prog).args(args).arg(&path).status();
            *term = ratatui::init();
            term.clear()?;
            if let Err(e) = st {
                app.status = format!("Couldn't launch editor '{prog}': {e}");
            } else {
                app.status = "Back from editor — press r to run".into();
            }
        }

        if app.quit {
            return Ok(());
        }
    }
}

fn main() -> io::Result<()> {
    let mut term = ratatui::init();
    let res = run(&mut term);
    ratatui::restore();
    res
}
