use crossterm::InputEvent;
use livesplit_core::component::{
    possible_time_save, previous_segment, splits, sum_of_best, timer, title,
};
use livesplit_core::run::parser::composite;
use livesplit_core::{
    settings::SemanticColor as LSColor, HotkeySystem, Run, Segment, SharedTimer, Timer,
};
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::sync::mpsc::channel;
use std::time::Duration;
use std::{io, thread};
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout as TuiLayout};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Paragraph, Row, Table, Text, Widget};
use tui::Terminal;

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
        .map_err(drop)
        .and_then(|f| composite::parse(BufReader::new(f), None, true).map_err(drop))
    {
        run.run
    } else {
        let mut run = Run::new();
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

    let timer = Timer::new(run).unwrap().into_shared();
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

    let mut terminal = Terminal::new(CrosstermBackend::new()).unwrap();
    terminal.clear().unwrap();
    terminal.hide_cursor().unwrap();

    let (tx, rx) = channel();

    timer.write().split_or_start();

    thread::spawn(move || {
        let input = crossterm::input().read_sync();
        for event in input {
            tx.send(()).unwrap();
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
    use livesplit_core::settings::SemanticColor::*;
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

fn draw(t: &mut Terminal<CrosstermBackend>, layout: &mut Layout) {
    let layout_settings = Default::default();
    let splits_state = layout
        .components
        .splits
        .state(&layout.timer.read(), &layout_settings);

    t.draw(|mut f| {
        let chunks = TuiLayout::default()
            .margin(1)
            .constraints(vec![
                Constraint::Length(3),
                Constraint::Length(splits_state.splits.len() as u16 + 3),
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .direction(Direction::Vertical)
            .split(f.size());

        let state = layout.components.title.state(&layout.timer.read());

        let category = format!("{:^35}", state.line2.unwrap_or_default());
        let attempts = state
            .attempts
            .map_or_else(|| format!("{:>35}", ""), |a| format!("{:>35}", a));
        let category: String = category
            .chars()
            .zip(attempts.chars())
            .map(|(c, a)| if a.is_whitespace() { c } else { a })
            .collect();

        Paragraph::new([Text::raw(&format!("{:^35}\n{}", state.line1, category))].iter())
            .render(&mut f, chunks[0]);

        let styles = splits_state
            .splits
            .iter()
            .map(|s| {
                if s.is_current_split {
                    Style::default().fg(Color::Rgb(77, 166, 255))
                } else {
                    Style::default().fg(map_color(LSColor::Default))
                }
            })
            .collect::<Vec<_>>();

        let splits = splits_state
            .splits
            .iter()
            .zip(styles.into_iter())
            .map(|(s, style)| {
                Row::StyledData(
                    vec![
                        s.name.clone(),
                        format!("{:>9}", s.columns.get(1).map_or("", |c| &c.value)),
                        format!("{:>9}", s.columns.get(0).map_or("", |c| &c.value)),
                    ]
                    .into_iter(),
                    style,
                )
            })
            .collect::<Vec<_>>();

        let mut labels = vec![String::from("Split")];

        for label in splits_state.column_labels.unwrap_or_default().iter().rev() {
            labels.push(format!("{:>9}", label));
        }

        Table::new(labels.into_iter(), splits.into_iter())
            .header_style(Style::default().fg(Color::White))
            .widths(&[15, 9, 9])
            .style(Style::default().fg(Color::White))
            .column_spacing(1)
            .render(&mut f, chunks[1]);

        let state = layout
            .components
            .timer
            .state(&layout.timer.read(), &layout_settings);

        Paragraph::new([Text::raw(format!("{:>32}{}", state.time, state.fraction))].iter())
            .style(
                Style::default()
                    .modifier(Modifier::BOLD)
                    .fg(map_color(state.semantic_color)),
            )
            .render(&mut f, chunks[2]);

        let state = layout
            .components
            .previous_segment
            .state(&layout.timer.read(), &layout_settings);

        Paragraph::new([Text::raw(format_info_text(&state.text, &state.time))].iter())
            .style(Style::default().fg(map_color(state.semantic_color)))
            .render(&mut f, chunks[3]);

        let state = layout.components.sum_of_best.state(&layout.timer.read());

        Paragraph::new([Text::raw(format_info_text(&state.text, &state.time))].iter())
            .style(Style::default().fg(Color::White))
            .render(&mut f, chunks[4]);

        let state = layout
            .components
            .possible_time_save
            .state(&layout.timer.read());

        Paragraph::new([Text::raw(format_info_text(&state.text, &state.time))].iter())
            .style(Style::default().fg(Color::White))
            .render(&mut f, chunks[5]);
    })
    .unwrap();
}

fn format_info_text(text: &str, value: &str) -> String {
    let text = format!("{:<35}", text);
    let value = format!("{:>35}", value);
    text.chars()
        .zip(value.chars())
        .map(|(t, v)| if v.is_whitespace() { t } else { v })
        .collect()
}
