//! Backend using the terminal library.
//!
//! Requires the `terminal-backend` feature.

#![cfg(feature = "terminal")]

use std::{cell::{Cell}, io::{self, BufWriter, Stdout, Write}, time::Duration};


use crate::{
    backend,
    event::{Event, Key, MouseButton, MouseEvent},
    theme,
    vec::Vec2,
};

use terminal::{Event as TEvent, MouseButton as TMouseButton, MouseEvent as TMouseEvent, KeyCode as TKeyCode, KeyEvent as TKeyEvent, KeyModifiers as TKeyModifiers, Color as TColor, Attribute as TAttribute, Terminal, Action, Value, Retrieved, Clear};
use std::fs::File;

impl From<TMouseButton> for MouseButton {
    fn from(button: TMouseButton) -> Self {
        match button {
            TMouseButton::Left => MouseButton::Left,
            TMouseButton::Right => MouseButton::Right,
            TMouseButton::Middle => MouseButton::Middle,
            TMouseButton::Unknown => { MouseButton::Other }
        }
    }
}

impl From<TKeyCode> for Key {
    fn from(code: TKeyCode) -> Self {
        match code {
            TKeyCode::Esc => Key::Esc,
            TKeyCode::Backspace => Key::Backspace,
            TKeyCode::Left => Key::Left,
            TKeyCode::Right => Key::Right,
            TKeyCode::Up => Key::Up,
            TKeyCode::Down => Key::Down,
            TKeyCode::Home => Key::Home,
            TKeyCode::End => Key::End,
            TKeyCode::PageUp => Key::PageUp,
            TKeyCode::PageDown => Key::PageDown,
            TKeyCode::Delete => Key::Del,
            TKeyCode::Insert => Key::Ins,
            TKeyCode::Enter => Key::Enter,
            TKeyCode::Tab => Key::Tab,
            TKeyCode::F(n) => Key::from_f(n),
            TKeyCode::BackTab => Key::Tab, /* not supported */
            TKeyCode::Char(_) => Key::Tab, /* is handled at `Event` level, use tab as default */
            TKeyCode::Null => Key::Tab, /* is handled at `Event` level, use tab as default */
        }
    }
}

impl From<TKeyEvent> for Event {
    fn from(event: TKeyEvent) -> Self {
        const CTRL_ALT: TKeyModifiers = TKeyModifiers::from_bits_truncate(
            TKeyModifiers::CONTROL.bits() | TKeyModifiers::ALT.bits(),
        );
        const CTRL_SHIFT: TKeyModifiers = TKeyModifiers::from_bits_truncate(
            TKeyModifiers::CONTROL.bits() | TKeyModifiers::SHIFT.bits(),
        );
        const ALT_SHIFT: TKeyModifiers = TKeyModifiers::from_bits_truncate(
            TKeyModifiers::ALT.bits() | TKeyModifiers::SHIFT.bits(),
        );

        match event {
            // Handle Char + modifier.
            TKeyEvent {
                modifiers: TKeyModifiers::CONTROL,
                code: TKeyCode::Char('c'),
            } => Event::Exit,
            TKeyEvent {
                modifiers: TKeyModifiers::CONTROL,
                code: TKeyCode::Char(c),
            } => Event::CtrlChar(c),
            TKeyEvent {
                modifiers: TKeyModifiers::ALT,
                code: TKeyCode::Char(c),
            } => Event::AltChar(c),
            TKeyEvent {
                modifiers: TKeyModifiers::SHIFT,
                code: TKeyCode::Char(c),
            } => Event::Char(c),

            // Handle key + multiple modifiers
            TKeyEvent {
                modifiers: CTRL_ALT,
                code,
            } => Event::CtrlAlt(Key::from(code)),
            TKeyEvent {
                modifiers: CTRL_SHIFT,
                code,
            } => Event::CtrlShift(Key::from(code)),
            TKeyEvent {
                modifiers: ALT_SHIFT,
                code,
            } => Event::AltShift(Key::from(code)),

            // Handle key + single modifier
            TKeyEvent {
                modifiers: TKeyModifiers::CONTROL,
                code,
            } => Event::Ctrl(Key::from(code)),
            TKeyEvent {
                modifiers: TKeyModifiers::ALT,
                code,
            } => Event::Alt(Key::from(code)),
            TKeyEvent {
                modifiers: TKeyModifiers::SHIFT,
                code,
            } => Event::Shift(Key::from(code)),

            TKeyEvent {
                code: TKeyCode::Char(c),
                ..
            } => Event::Char(c),

            // Explicitly handle 'baTktab' since terminal does not sent SHIFT alongside the baTk tab key.
            TKeyEvent {
                code: TKeyCode::BackTab,
                ..
            } => Event::Shift(Key::Tab),

            // All other keys.
            TKeyEvent { code, .. } => Event::Key(Key::from(code)),
        }
    }
}

impl From<theme::Color> for TColor {
    fn from(base_color: theme::Color) -> Self {
        match base_color {
            theme::Color::Dark(theme::BaseColor::Black) => TColor::Black,
            theme::Color::Dark(theme::BaseColor::Red) => TColor::DarkRed,
            theme::Color::Dark(theme::BaseColor::Green) => TColor::DarkGreen,
            theme::Color::Dark(theme::BaseColor::Yellow) => TColor::DarkYellow,
            theme::Color::Dark(theme::BaseColor::Blue) => TColor::DarkBlue,
            theme::Color::Dark(theme::BaseColor::Magenta) => {
                TColor::DarkMagenta
            }
            theme::Color::Dark(theme::BaseColor::Cyan) => TColor::DarkCyan,
            theme::Color::Dark(theme::BaseColor::White) => TColor::Grey,
            theme::Color::Light(theme::BaseColor::Black) => TColor::Grey,
            theme::Color::Light(theme::BaseColor::Red) => TColor::Red,
            theme::Color::Light(theme::BaseColor::Green) => TColor::Green,
            theme::Color::Light(theme::BaseColor::Yellow) => TColor::Yellow,
            theme::Color::Light(theme::BaseColor::Blue) => TColor::Blue,
            theme::Color::Light(theme::BaseColor::Magenta) => TColor::Magenta,
            theme::Color::Light(theme::BaseColor::Cyan) => TColor::Cyan,
            theme::Color::Light(theme::BaseColor::White) => TColor::White,
            theme::Color::Rgb(r, g, b) => TColor::Rgb(r, g, b),
            theme::Color::RgbLowRes(r, g, b) => {
                debug_assert!(r <= 5,
                              "Red color fragment (r = {}) is out of bound. Make sure r ≤ 5.",
                              r);
                debug_assert!(g <= 5,
                              "Green color fragment (g = {}) is out of bound. Make sure g ≤ 5.",
                              g);
                debug_assert!(b <= 5,
                              "Blue color fragment (b = {}) is out of bound. Make sure b ≤ 5.",
                              b);

                TColor::AnsiValue(16 + 36 * r + 6 * g + b)
            }
            theme::Color::TerminalDefault => TColor::Reset,
        }
    }
}

/// Backend using terminal-backend
pub struct Backend {
    current_style: Cell<theme::ColorPair>,
    last_button: Option<MouseButton>,
    terminal: Terminal<File>
}


impl Backend {
    /// Creates a new terminal backend.
    pub fn init() -> io::Result<Box<dyn backend::Backend>>
        where
            Self: Sized,
    {
        let terminal = Terminal::custom(File::open("/dev/tty").unwrap());
        terminal.act(Action::EnterAlternateScreen);
        terminal.act(Action::EnableRawMode).unwrap();
        terminal.act(Action::HideCursor).unwrap();

        Ok(Box::new(Backend {
            current_style: Cell::new(theme::ColorPair::from_256colors(0, 0)),
            last_button: None,
            terminal
        }))
    }

    fn apply_colors(&self, colors: theme::ColorPair) {
        self.terminal.act(Action::SetForegroundColor(TColor::from(colors.front))).unwrap();
        self.terminal.act(Action::SetBackgroundColor(TColor::from(colors.back))).unwrap();
    }

    fn set_attr(&self, attr: TAttribute) {
        self.terminal.act(Action::SetAttribute(attr)).unwrap();
    }

    fn map_key(&mut self, event: TEvent) -> Event {
        match event {
            TEvent::Key(key_event) => Event::from(key_event),
            TEvent::Mouse(mouse_event) => {
                let position;
                let event;

                match mouse_event {
                    TMouseEvent::Down(button, x, y, _) => {
                        let button = MouseButton::from(button);
                        self.last_button = Some(button);
                        event = MouseEvent::Press(button);
                        position = (x, y).into();
                    }
                    TMouseEvent::Up(_, x, y, _) => {
                        event = MouseEvent::Release(self.last_button.unwrap());
                        position = (x, y).into();
                    }
                    TMouseEvent::Drag(_, x, y, _) => {
                        event = MouseEvent::Hold(self.last_button.unwrap());
                        position = (x, y).into();
                    }
                    TMouseEvent::ScrollDown(x, y, _) => {
                        event = MouseEvent::WheelDown;
                        position = (x, y).into();
                    }
                    TMouseEvent::ScrollUp(x, y, _) => {
                        event = MouseEvent::WheelDown;
                        position = (x, y).into();
                    }
                };

                Event::Mouse {
                    event,
                    position,
                    offset: Vec2::zero(),
                }
            }
            TEvent::Resize => Event::WindowResize,
            TEvent::Unknown => Event::Unknown(vec![])
        }
    }
}

impl backend::Backend for Backend {
    fn poll_event(&mut self) -> Option<Event> {
        match self.terminal.get(Value::Event(None)).unwrap() {
            Retrieved::Event(Some(event)) => Some(self.map_key(event)),
            _ => None
        }
    }

    fn finish(&mut self) {
        self.terminal.act(Action::LeaveAlternateScreen).unwrap();
        self.terminal.act(Action::ShowCursor).unwrap();
        self.terminal.act(Action::DisableRawMode).unwrap();
        self.terminal.act(Action::ResetColor).unwrap();
    }

    fn refresh(&mut self) {
        self.terminal.flush_batch().unwrap();
    }

    fn has_colors(&self) -> bool {
        // TODO: color support detection?
        true
    }

    fn screen_size(&self) -> Vec2 {
        if let Retrieved::TerminalSize(x, y) =  self.terminal.get(Value::TerminalSize).unwrap().into() {
            Vec2::new(x as usize, y as usize)
        }else {
            panic!("Not possible.");
        }
    }

    fn print_at(&self, pos: Vec2, text: &str) {
        let mut lock = self.terminal.lock_mut().unwrap();
        lock.act(Action::MoveCursorTo(pos.x as u16, pos.y as u16)).unwrap();
        lock.write(text.as_bytes()).unwrap();
        lock.flush_batch().unwrap();
    }

    fn print_at_rep(&self, pos: Vec2, repetitions: usize, text: &str) {
        if repetitions > 0 {
            let mut lock = self.terminal.lock_mut().unwrap();
            lock.batch(Action::MoveCursorTo(pos.x as u16, pos.y as u16)).unwrap();
            lock.write_all(text.as_bytes()).unwrap();

            let mut dupes_left = repetitions - 1;
            while dupes_left > 0 {
                lock.write_all(text.as_bytes()).unwrap();
                dupes_left -= 1;
            }
        }
    }

    fn clear(&self, color: theme::Color) {
        self.apply_colors(theme::ColorPair {
            front: color,
            back: color,
        });

        self.terminal.act(Action::ClearTerminal(Clear::All)).unwrap();
    }

    fn set_color(&self, color: theme::ColorPair) -> theme::ColorPair {
        let current_style = self.current_style.get();

        if current_style != color {
            self.apply_colors(color);
            self.current_style.set(color);
        }

        current_style
    }

    fn set_effect(&self, effect: theme::Effect) {
        match effect {
            theme::Effect::Simple => (),
            theme::Effect::Reverse => self.set_attr(TAttribute::Reversed),
            theme::Effect::Bold => self.set_attr(TAttribute::Bold),
            theme::Effect::Italic => self.set_attr(TAttribute::Italic),
            theme::Effect::Underline => self.set_attr(TAttribute::Underlined),
        }
    }

    fn unset_effect(&self, effect: theme::Effect) {
        match effect {
            theme::Effect::Simple => (),
            theme::Effect::Reverse => self.set_attr(TAttribute::ReversedOff),
            theme::Effect::Bold => self.set_attr(TAttribute::NormalIntensity),
            theme::Effect::Italic => self.set_attr(TAttribute::ItalicOff),
            theme::Effect::Underline => self.set_attr(TAttribute::UnderlinedOff),
        }
    }

    fn name(&self) -> &str {
        "terminal"
    }
}