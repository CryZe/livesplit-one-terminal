#![feature(io)]

extern crate tui;
extern crate livesplit_core;

use tui::Terminal;
use tui::backend::TermionBackend;
use tui::layout::{Group, Direction, Size};
use tui::widgets::{Table, Widget, Paragraph};
use tui::style::{Color, Style, Modifier};
use livesplit_core::{Timer, Run, Segment, HotkeySystem, SharedTimer, Color as LSColor};
use livesplit_core::component::{timer, splits, title, previous_segment, sum_of_best,
                                possible_time_save};
use livesplit_core::parser::composite;
use std::{thread, io};
use std::io::prelude::*;
use std::io::BufReader;
use std::time::Duration;
use std::sync::mpsc::channel;
use std::fs::File;

struct Layout {
    timer: SharedTimer,
    components: Components,
}

struct Components {
    timer: timer::Component,
    splits: splits::Component,
    title: title::Component,
    previous_segment: previous_segment::Component,
    sum_of_best: sum_of_best::Component,
    possible_time_save: possible_time_save::Component,
}

fn main() {
    let run = if let Ok(run) = File::open("splits.lss")
        .map_err(|_| ())
        .and_then(|f| composite::parse(BufReader::new(f), None, true).map_err(|_| ())) {
        run
    } else {
        let mut run = Run::new(Vec::new());
        run.set_game_name("Breath of the Wild");
        run.set_category_name("Any%");

        run.push_segment(Segment::new("Shrine 1"));
        run.push_segment(Segment::new("Shrine 2"));
        run.push_segment(Segment::new("Shrine 3"));
        run.push_segment(Segment::new("Shrine 4"));
        run.push_segment(Segment::new("Glider"));
        run.push_segment(Segment::new("Ganon"));

        run
    };

    let timer = Timer::new(run).into_shared();
    let _hotkey_system = HotkeySystem::new(timer.clone()).ok();

    let mut layout = Layout {
        timer: timer.clone(),
        components: Components {
            timer: timer::Component::new(),
            splits: splits::Component::new(),
            title: title::Component::new(),
            previous_segment: previous_segment::Component::new(),
            sum_of_best: sum_of_best::Component::new(),
            possible_time_save: possible_time_save::Component::new(),
        },
    };

    let mut terminal = Terminal::new(TermionBackend::new().unwrap()).unwrap();
    terminal.clear().unwrap();
    terminal.hide_cursor().unwrap();

    let (tx, rx) = channel();

    thread::spawn(move || {
        let stdin = io::stdin();
        for c in stdin.lock().chars() {
            let c = c.unwrap();
            match c {
                'q' => break,
                '1' => timer.write().split(),
                '2' => timer.write().skip_split(),
                '3' => timer.write().reset(true),
                '4' => timer.write().switch_to_previous_comparison(),
                '5' => timer.write().pause(),
                '6' => timer.write().switch_to_next_comparison(),
                '8' => timer.write().undo_split(),
                _ => {}
            }
        }
        tx.send(()).unwrap();
    });

    loop {
        if let Ok(_) = rx.try_recv() {
            break;
        }

        draw(&mut terminal, &mut layout);
        thread::sleep(Duration::from_millis(33));
    }

    terminal.clear().unwrap();
    terminal.show_cursor().unwrap();
}

fn map_color(color: LSColor) -> Color {
    use livesplit_core::Color::*;
    match color {
        AheadGainingTime => Color::Rgb(0x00, 0xCC, 0x4B),
        AheadLosingTime => Color::Rgb(0x5C, 0xD6, 0x89),
        BehindGainingTime => Color::Rgb(0xD6, 0x5C, 0x5C),
        BehindLosingTime => Color::Rgb(0xCC, 0x00, 0x00),
        BestSegment => Color::Rgb(0xFF, 0xD5, 0x00),
        NotRunning => Color::Rgb(0x99, 0x99, 0x99),
        Paused => Color::Rgb(0x66, 0x66, 0x66),
        PersonalBest => Color::Rgb(0x4D, 0xA6, 0xFF),
        Default => Color::White,
    }
}

fn draw(t: &mut Terminal<TermionBackend>, layout: &mut Layout) {
    let size = t.size().unwrap();

    let splits_state = layout.components.splits.state(&layout.timer.read());

    Group::default()
        .margin(1)
        .sizes(&[Size::Fixed(3),
                 Size::Fixed(splits_state.splits.len() as u16 + 3),
                 Size::Fixed(2),
                 Size::Fixed(1),
                 Size::Fixed(1),
                 Size::Fixed(1)])
        .direction(Direction::Vertical)
        .render(t, &size, |t, chunks| {
            let state = layout.components.title.state(&layout.timer.read());

            let category = format!("{:^35}", state.category);
            let attempts = format!("{:>35}", state.attempts);
            let category: String = category.chars()
                .zip(attempts.chars())
                .map(|(c, a)| if a.is_whitespace() { c } else { a })
                .collect();

            Paragraph::default()
                .text(&format!("{:^35}\n{}", state.game, category))
                .render(t, &chunks[0]);

            let styles = splits_state.splits
                .iter()
                .map(|s| if s.is_current_split {
                    Style::default().fg(Color::Rgb(77, 166, 255))
                } else {
                    Style::default().fg(map_color(s.color))
                })
                .collect::<Vec<_>>();

            let splits = splits_state.splits
                .iter()
                .zip(styles.iter())
                .map(|(s, style)| {
                    ([s.name.clone(), format!("{:>9}", s.delta), format!("{:>9}", s.time)], style)
                })
                .collect::<Vec<_>>();

            Table::default()
                .header(&["Split", "    Delta", "     Time"])
                .header_style(Style::default().fg(Color::White))
                .widths(&[15, 9, 9])
                .style(Style::default().fg(Color::White))
                .column_spacing(1)
                .rows(&splits)
                .render(t, &chunks[1]);

            let state = layout.components.timer.state(&layout.timer.read());

            Paragraph::default()
                .text(&format!("{:>32}{}", state.time, state.fraction))
                .style(Style::default().modifier(Modifier::Bold).fg(map_color(state.color)))
                .render(t, &chunks[2]);

            let state = layout.components.previous_segment.state(&layout.timer.read());

            Paragraph::default()
                .text(&format_info_text(&state.text, &state.time))
                .style(Style::default().fg(map_color(state.color)))
                .render(t, &chunks[3]);

            let state = layout.components.sum_of_best.state(&layout.timer.read());

            Paragraph::default()
                .text(&format_info_text(&state.text, &state.time))
                .style(Style::default().fg(Color::White))
                .render(t, &chunks[4]);

            let state = layout.components.possible_time_save.state(&layout.timer.read());

            Paragraph::default()
                .text(&format_info_text(&state.text, &state.time))
                .style(Style::default().fg(Color::White))
                .render(t, &chunks[5]);
        });

    t.draw().unwrap();
}

fn format_info_text(text: &str, value: &str) -> String {
    let text = format!("{:<35}", text);
    let value = format!("{:>35}", value);
    text.chars()
        .zip(value.chars())
        .map(|(t, v)| if v.is_whitespace() { t } else { v })
        .collect()
}
