use crossterm::{
    execute, input, queue, EnterAlternateScreen, Goto, Hide, InputEvent, KeyEvent,
    LeaveAlternateScreen, Output, RawScreen, ResetColor, SetBg, SetFg, Show,
};
use livesplit_core::component::{
    possible_time_save, previous_segment, splits, sum_of_best, timer, title,
};
use livesplit_core::run::parser::composite;
use livesplit_core::{
    layout::{ComponentState, Layout},
    settings::{Color, Gradient},
    HotkeySystem, Run, Segment, SharedTimer, Timer,
};
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::sync::mpsc::channel;
use std::time::Duration;
use std::{io, thread};

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

    let mut layout = Layout::default_layout();

    let timer = Timer::new(run).unwrap().into_shared();
    let _hotkey_system = HotkeySystem::new(timer.clone()).ok();

    let _raw = RawScreen::into_raw_mode();

    let stdout = io::stdout();
    let mut stdout = BufWriter::with_capacity(128 << 10, stdout.lock());
    queue!(stdout, Hide, EnterAlternateScreen).unwrap();
    stdout.flush().unwrap();

    let mut inputs = input().read_async();

    'main_loop: loop {
        for input in inputs.by_ref() {
            match input {
                InputEvent::Keyboard(key) => match key {
                    KeyEvent::Ctrl('c') | KeyEvent::Esc => break 'main_loop,
                    KeyEvent::Char('1') => timer.write().split_or_start(),
                    KeyEvent::Char('2') => timer.write().skip_split(),
                    KeyEvent::Char('3') => timer.write().reset(true),
                    KeyEvent::Char('4') => timer.write().switch_to_previous_comparison(),
                    KeyEvent::Char('5') => timer.write().toggle_pause(),
                    KeyEvent::Char('6') => timer.write().switch_to_next_comparison(),
                    KeyEvent::Char('8') => timer.write().undo_split(),
                    KeyEvent::Up => layout.scroll_up(),
                    KeyEvent::Down => layout.scroll_down(),
                    _ => {}
                },
                _ => {}
            }
        }

        queue!(stdout, Goto(0, 0)).unwrap();

        let state = layout.state(&timer.read());

        let mut row_index = 0;
        for component in state.components {
            match component {
                ComponentState::Title(c) => {
                    write!(stdout, "{:^40}", c.line1).unwrap();
                    row_index += 1;
                    queue!(stdout, Goto(0, row_index)).unwrap();
                    if let Some(line2) = c.line2 {
                        write!(stdout, "{:^40}", line2).unwrap();
                    }
                    row_index += 1;
                    queue!(stdout, Goto(0, row_index)).unwrap();
                }
                ComponentState::Timer(mut c) => {
                    queue!(stdout, SetFg(convert_color(c.top_color))).unwrap();
                    c.time.push_str(&c.fraction);
                    write!(stdout, "{:>40}", c.time).unwrap();
                    row_index += 1;
                    queue!(stdout, ResetColor, Goto(0, row_index)).unwrap();
                }
                ComponentState::KeyValue(c) => {
                    if c.display_two_rows {
                        row_index += 1;
                        queue!(stdout, Goto(0, row_index)).unwrap();
                    }
                    if let Some(color) = c.value_color {
                        queue!(stdout, SetFg(convert_color(color))).unwrap();
                    }
                    write!(stdout, "{:>40}", c.value).unwrap();
                    if c.display_two_rows {
                        row_index -= 1;
                    }
                    queue!(
                        stdout,
                        Goto(0, row_index),
                        SetFg(convert_color(c.key_color.unwrap_or(state.text_color)))
                    )
                    .unwrap();
                    write!(stdout, "{}", c.key).unwrap();
                    if c.display_two_rows {
                        row_index += 2;
                    } else {
                        row_index += 1;
                    }
                    queue!(
                        stdout,
                        Goto(0, row_index),
                        SetFg(convert_color(state.text_color))
                    )
                    .unwrap();
                }
                ComponentState::Splits(c) => {
                    if let Some(labels) = c.column_labels {
                        let mut col = 40;
                        for label in labels {
                            col -= 9;
                            queue!(stdout, Goto(col, row_index)).unwrap();
                            write!(stdout, "{:>9}", label).unwrap();
                        }
                    }
                    for split in c.splits {
                        if split.is_current_split {
                            queue!(stdout, SetBg(convert_gradient(c.current_split_gradient)))
                                .unwrap();
                        }
                        write!(stdout, "{:>40}", "").unwrap();
                        let mut col = 40;
                        for column in split.columns {
                            col -= 9;
                            queue!(
                                stdout,
                                Goto(col, row_index),
                                SetFg(convert_color(column.visual_color))
                            )
                            .unwrap();
                            write!(stdout, "{:>9}", column.value).unwrap();
                        }
                        queue!(
                            stdout,
                            Goto(0, row_index),
                            SetFg(convert_color(state.text_color))
                        )
                        .unwrap();
                        write!(stdout, "{}", split.name).unwrap();
                        if split.is_current_split {
                            queue!(stdout, ResetColor).unwrap();
                        }
                        row_index += 1;
                        queue!(
                            stdout,
                            Goto(0, row_index),
                            SetFg(convert_color(state.text_color))
                        )
                        .unwrap();
                    }
                }
                _ => {}
            }
        }

        stdout.flush().unwrap();

        thread::sleep(Duration::from_secs(1) / 30);
    }

    execute!(stdout, LeaveAlternateScreen, Show).unwrap();
}

fn convert_gradient(gradient: Gradient) -> crossterm::Color {
    match gradient {
        Gradient::Transparent => unimplemented!(),
        Gradient::Horizontal(l, _r) => convert_color(l),
        Gradient::Vertical(t, _b) => convert_color(t),
        Gradient::Plain(c) => convert_color(c),
    }
}

fn convert_color(color: Color) -> crossterm::Color {
    crossterm::Color::Rgb {
        r: (color.rgba.color.red * 255.0).round() as u8,
        g: (color.rgba.color.green * 255.0).round() as u8,
        b: (color.rgba.color.blue * 255.0).round() as u8,
    }
}
