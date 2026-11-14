use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::{days_in, opens_text, App, Mode, Pane, Prompt, Tab};
use crate::render;

fn centered(area: Rect, pw: u16, ph: u16) -> Rect {
    let w = area.width * pw / 100;
    let h = (area.height * ph / 100).max(3);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

pub fn ui(f: &mut Frame, app: &mut App) {
    let [title, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(2)])
            .areas(f.area());

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ★ Advent of Code ",
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" lang: {} (.{}) ", app.cfg.cur().name, app.cfg.cur().ext),
                Style::new().fg(Color::Magenta),
            ),
            Span::styled(
                format!(" data: {}  ", app.cfg.root.display()),
                Style::new().fg(Color::DarkGray),
            ),
        ])),
        title,
    );

    if app.view.is_some() {
        draw_day(f, app, body);
    } else {
        draw_picker(f, app, body);
    }

    let hints = if app.view.is_some() {
        "q back · p/i/o tabs · j/k scroll · e edit · y copy · r run · 1/2 submit · R refresh · [ ] prev/next day · L language · S session"
    } else {
        "↑↓ move · ←→/Tab switch pane · Enter open · L language · S session cookie · q quit"
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                app.status.clone(),
                Style::new().fg(Color::Yellow),
            )),
            Line::from(Span::styled(hints, Style::new().fg(Color::DarkGray))),
        ]),
        footer,
    );

    match &app.mode {
        Mode::Normal => {}
        Mode::Lang(sel) => {
            let a = f.area();
            let w = 90.min(a.width);
            let h = (app.cfg.langs.len() as u16 + 2).min(a.height);
            let r = Rect::new(a.x + (a.width - w) / 2, a.y + (a.height - h) / 2, w, h);
            let items: Vec<ListItem> = app
                .cfg
                .langs
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let cur = if i == app.cfg.cur { " ✔" } else { "" };
                    ListItem::new(format!(
                        "{}. {}{}   .{}   {}",
                        i + 1,
                        l.name,
                        cur,
                        l.ext,
                        l.cmd
                    ))
                })
                .collect();
            let mut st = ListState::default();
            st.select(Some(*sel));
            f.render_widget(Clear, r);
            f.render_stateful_widget(
                List::new(items)
                    .block(Block::default().borders(Borders::ALL).title(format!(
                        " Language — Enter/1-9 choose, Esc cancel · edit {} ",
                        app.cfg.config_file().display()
                    )))
                    .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
                    .highlight_symbol("▶ "),
                r,
                &mut st,
            );
        }
        Mode::Popup(text) => {
            let r = centered(f.area(), 70, 30);
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(text.clone())
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Result — press any key "),
                    ),
                r,
            );
        }
        Mode::Prompt { kind, buf } => {
            let title = match kind {
                Prompt::Session => {
                    " Paste your adventofcode.com session cookie (Enter to save, Esc to cancel) "
                        .to_string()
                }
                Prompt::Submit(l) => {
                    format!(" Submit answer for part {l} (Enter to send, Esc to cancel) ")
                }
            };
            let r = centered(f.area(), 80, 10);
            let r = Rect { height: 3, ..r };
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(format!("{buf}█"))
                    .block(Block::default().borders(Borders::ALL).title(title)),
                r,
            );
        }
    }
}

fn draw_picker(f: &mut Frame, app: &mut App, area: Rect) {
    let [l, r] = Layout::horizontal([Constraint::Length(20), Constraint::Min(24)]).areas(area);
    let focus = |p: Pane| {
        if app.pane == p {
            Color::Yellow
        } else {
            Color::DarkGray
        }
    };
    let (yc, dc) = (focus(Pane::Years), focus(Pane::Days));
    let y = app.year();

    let year_items: Vec<ListItem> = app
        .years
        .iter()
        .map(|y| {
            let s = app
                .stars
                .get(y)
                .map(|m| m.values().map(|&v| v as u32).sum::<u32>());
            ListItem::new(match s {
                Some(n) => format!("{y}  ★ {n}"),
                None if !app.started(*y) => format!("{y}  (soon)"),
                None => format!("{y}"),
            })
        })
        .collect();

    let m = app.stars.get(&y);
    let day_items: Vec<ListItem> = (1..=days_in(y))
        .map(|d| {
            let st = m.and_then(|m| m.get(&d)).copied().unwrap_or(0) as usize;
            let locked = !app.unlocked(y, d);
            let mut spans = vec![Span::raw(format!("Day {d:>2}  "))];
            spans.push(Span::styled(
                "★".repeat(st),
                Style::new().fg(Color::Yellow),
            ));
            if st < 2 && !locked {
                spans.push(Span::styled(
                    "☆".repeat(2 - st),
                    Style::new().fg(Color::DarkGray),
                ));
            }
            if locked {
                spans.push(Span::raw("locked"));
                return ListItem::new(Line::from(spans)).style(Style::new().fg(Color::DarkGray));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let hl = Style::new().add_modifier(Modifier::REVERSED);
    f.render_stateful_widget(
        List::new(year_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Years ")
                    .border_style(Style::new().fg(yc)),
            )
            .highlight_style(hl)
            .highlight_symbol("▶ "),
        l,
        &mut app.year_state,
    );
    let day_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {y} "))
        .border_style(Style::new().fg(dc));
    if !app.started(y) {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  Advent of Code {y} has not started yet."),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(format!("  {}", opens_text(y))),
                Line::from(""),
                Line::from(Span::styled(
                    "  Pick an earlier year in the Years list to start solving.",
                    Style::new().fg(Color::DarkGray),
                )),
            ])
            .wrap(Wrap { trim: false })
            .block(day_block),
            r,
        );
    } else {
        f.render_stateful_widget(
            List::new(day_items)
                .block(day_block)
                .highlight_style(hl)
                .highlight_symbol("▶ "),
            r,
            &mut app.day_state,
        );
    }
}

fn draw_day(f: &mut Frame, app: &mut App, area: Rect) {
    let [tabs_a, body_a] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    let (tab, year, day) = {
        let v = app.view.as_ref().unwrap();
        (v.tab, v.year, v.day)
    };
    f.render_widget(
        Tabs::new(vec!["Puzzle (p)", "Input (i)", "Output (o)"])
            .select(tab as usize)
            .highlight_style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .divider("│"),
        tabs_a,
    );
    let block = Block::default().borders(Borders::ALL).title(format!(
        " {year} · Day {day} · {} (.{}) ",
        app.cfg.cur().name,
        app.cfg.cur().ext
    ));
    let inner = block.inner(body_a);
    let (w, h) = (inner.width as usize, inner.height as usize);
    app.page = h;

    let (lines, total, sc, pscroll): (Vec<Line<'static>>, usize, usize, u16) = {
        let v = app.view.as_ref().unwrap();
        let want = v.scroll[tab as usize];
        match tab {
            Tab::Puzzle => match &v.desc {
                Some(d) => {
                    let l = render::render_html(d, w.saturating_sub(2));
                    let n = l.len();
                    let sc = want.min(n.saturating_sub(h));
                    (l, n, sc, sc as u16)
                }
                None => (vec![Line::from("Loading puzzle…")], 1, 0, 0),
            },
            Tab::Input => match &v.input {
                Some(s) => {
                    let n = s.lines().count();
                    let sc = want.min(n.saturating_sub(h));
                    let l = s
                        .lines()
                        .skip(sc)
                        .take(h)
                        .map(|l| Line::from(l.to_string()))
                        .collect();
                    (l, n, sc, 0)
                }
                None => (
                    vec![Line::from(
                        "No input yet (needs a session cookie — press S)",
                    )],
                    1,
                    0,
                    0,
                ),
            },
            Tab::Output => match &v.output {
                Some(s) => {
                    let all: Vec<Line<'static>> =
                        s.lines().map(|l| Line::from(l.to_string())).collect();
                    let n = all.len();
                    let sc = want.min(n.saturating_sub(h));
                    (all, n, sc, sc as u16)
                }
                None => (
                    vec![Line::from(
                        "Nothing run yet — press e to write a solution, r to run it",
                    )],
                    1,
                    0,
                    0,
                ),
            },
        }
    };
    app.view.as_mut().unwrap().scroll[tab as usize] = sc;
    let block = block.title_bottom(format!(" {}/{} ", (sc + 1).min(total), total));
    f.render_widget(
        Paragraph::new(lines).block(block).scroll((pscroll, 0)),
        body_a,
    );
}
