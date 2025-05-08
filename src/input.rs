use std::{error::Error, fmt::Display};

use ahash::AHashMap;
use bitflags::bitflags;
use egui::Pos2;
use serde::{Deserialize, Serialize};
use serde_yml as yml;
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, KeyEvent, MouseButton},
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey, SmolStr},
};

bitflags! {

    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct ActionFlags: u64 {
        const PRIMARY_INTERACT = 0;
        const SECONDARY_INTERACT = 1;
        const PAUSE = 2;
        const LOOK = 4;
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u64)]
pub enum Action {
    PrimaryInteract = ActionFlags::PRIMARY_INTERACT.bits(),
    SecondaryInteract = ActionFlags::SECONDARY_INTERACT.bits(),
    Pause = ActionFlags::PAUSE.bits(),
    Look(f32, f32) = ActionFlags::LOOK.bits(),
}
impl Action {
    fn from_bind_out(bind_out: BindOut) -> Option<Self> {
        match bind_out {
            BindOut::PrimaryInteract => Some(Self::PrimaryInteract),
            BindOut::SecondaryInteract => Some(Self::SecondaryInteract),
            BindOut::Pause => Some(Self::Pause),
            _ => None,
        }
    }
    pub fn discriminant(&self) -> ActionFlags {
        //Safe according to https://doc.rust-lang.org/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        unsafe { *<*const _>::from(self).cast::<ActionFlags>() }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum BindOut {
    PrimaryInteract,
    SecondaryInteract,
    Pause,
    MoveForward,
    MoveLeft,
    MoveRight,
    MoveBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum BindType {
    Key(KeyCode),
    KeyMod(KeyCode, ModifiersState),
    Named(NamedKey),
    Character(SmolStr),
    MouseButton(MouseButton),
    MouseMove,
}

#[derive(Debug)]
pub enum BindsErr {
    Yaml(yml::Error),
    IO(std::io::Error),
}
impl Display for BindsErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yaml(err) => writeln!(f, "Yaml error: {err}"),
            Self::IO(err) => writeln!(f, "IO error: {err}"),
        }
    }
}
impl Error for BindsErr {}
impl From<yml::Error> for BindsErr {
    fn from(value: yml::Error) -> Self {
        Self::Yaml(value)
    }
}
impl From<std::io::Error> for BindsErr {
    fn from(value: std::io::Error) -> Self {
        Self::IO(value)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Bindings(AHashMap<BindType, BindOut>);
impl Bindings {
    fn new() -> Result<Self, BindsErr> {
        if let Ok(binds_file) = std::fs::File::open("binds.yaml") {
            return Ok(yml::from_reader(binds_file)?);
        } else {
            log::info!("Bindings file not found, initializing with default binds");
        }
        let me = Self::default_binds();
        me.save()?;
        Ok(me)
    }
    fn default_binds() -> Self {
        Self(AHashMap::from([
            (BindType::Key(KeyCode::KeyW), BindOut::MoveForward),
            (BindType::Key(KeyCode::KeyA), BindOut::MoveLeft),
            (BindType::Key(KeyCode::KeyD), BindOut::MoveRight),
            (BindType::Key(KeyCode::KeyS), BindOut::MoveBack),
            (
                BindType::MouseButton(MouseButton::Left),
                BindOut::PrimaryInteract,
            ),
            (
                BindType::MouseButton(MouseButton::Right),
                BindOut::SecondaryInteract,
            ),
        ]))
    }
    fn save(&self) -> Result<(), BindsErr> {
        let file = std::fs::File::create("binds.yaml")?;
        yml::to_writer(&file, self)?;
        Ok(())
    }
}

//taken from egui_winit
fn key_from_key_code(key: winit::keyboard::KeyCode) -> Option<egui::Key> {
    use egui::Key;
    use winit::keyboard::KeyCode;

    Some(match key {
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,

        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,

        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,

        // Punctuation
        KeyCode::Space => Key::Space,
        KeyCode::Comma => Key::Comma,
        KeyCode::Period => Key::Period,
        // KeyCode::Colon => Key::Colon, // NOTE: there is no physical colon key on an american keyboard
        KeyCode::Semicolon => Key::Semicolon,
        KeyCode::Backslash => Key::Backslash,
        KeyCode::Slash | KeyCode::NumpadDivide => Key::Slash,
        KeyCode::BracketLeft => Key::OpenBracket,
        KeyCode::BracketRight => Key::CloseBracket,
        KeyCode::Backquote => Key::Backtick,
        KeyCode::Quote => Key::Quote,

        KeyCode::Cut => Key::Cut,
        KeyCode::Copy => Key::Copy,
        KeyCode::Paste => Key::Paste,
        KeyCode::Minus | KeyCode::NumpadSubtract => Key::Minus,
        KeyCode::NumpadAdd => Key::Plus,
        KeyCode::Equal => Key::Equals,

        KeyCode::Digit0 | KeyCode::Numpad0 => Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Key::Num9,

        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,

        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::F13 => Key::F13,
        KeyCode::F14 => Key::F14,
        KeyCode::F15 => Key::F15,
        KeyCode::F16 => Key::F16,
        KeyCode::F17 => Key::F17,
        KeyCode::F18 => Key::F18,
        KeyCode::F19 => Key::F19,
        KeyCode::F20 => Key::F20,
        KeyCode::F21 => Key::F21,
        KeyCode::F22 => Key::F22,
        KeyCode::F23 => Key::F23,
        KeyCode::F24 => Key::F24,
        KeyCode::F25 => Key::F25,
        KeyCode::F26 => Key::F26,
        KeyCode::F27 => Key::F27,
        KeyCode::F28 => Key::F28,
        KeyCode::F29 => Key::F29,
        KeyCode::F30 => Key::F30,
        KeyCode::F31 => Key::F31,
        KeyCode::F32 => Key::F32,
        KeyCode::F33 => Key::F33,
        KeyCode::F34 => Key::F34,
        KeyCode::F35 => Key::F35,

        _ => {
            return None;
        }
    })
}

fn key_from_named_key(named_key: winit::keyboard::NamedKey) -> Option<egui::Key> {
    use egui::Key;
    use winit::keyboard::NamedKey;

    Some(match named_key {
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::End => Key::End,
        NamedKey::Home => Key::Home,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert => Key::Insert,
        NamedKey::Escape => Key::Escape,
        NamedKey::Cut => Key::Cut,
        NamedKey::Copy => Key::Copy,
        NamedKey::Paste => Key::Paste,

        NamedKey::Space => Key::Space,

        NamedKey::F1 => Key::F1,
        NamedKey::F2 => Key::F2,
        NamedKey::F3 => Key::F3,
        NamedKey::F4 => Key::F4,
        NamedKey::F5 => Key::F5,
        NamedKey::F6 => Key::F6,
        NamedKey::F7 => Key::F7,
        NamedKey::F8 => Key::F8,
        NamedKey::F9 => Key::F9,
        NamedKey::F10 => Key::F10,
        NamedKey::F11 => Key::F11,
        NamedKey::F12 => Key::F12,
        NamedKey::F13 => Key::F13,
        NamedKey::F14 => Key::F14,
        NamedKey::F15 => Key::F15,
        NamedKey::F16 => Key::F16,
        NamedKey::F17 => Key::F17,
        NamedKey::F18 => Key::F18,
        NamedKey::F19 => Key::F19,
        NamedKey::F20 => Key::F20,
        NamedKey::F21 => Key::F21,
        NamedKey::F22 => Key::F22,
        NamedKey::F23 => Key::F23,
        NamedKey::F24 => Key::F24,
        NamedKey::F25 => Key::F25,
        NamedKey::F26 => Key::F26,
        NamedKey::F27 => Key::F27,
        NamedKey::F28 => Key::F28,
        NamedKey::F29 => Key::F29,
        NamedKey::F30 => Key::F30,
        NamedKey::F31 => Key::F31,
        NamedKey::F32 => Key::F32,
        NamedKey::F33 => Key::F33,
        NamedKey::F34 => Key::F34,
        NamedKey::F35 => Key::F35,
        _ => {
            log::trace!("Unknown key: {named_key:?}");
            return None;
        }
    })
}

fn mouse_button_to_pointer_button(mb: MouseButton) -> Option<egui::PointerButton> {
    match mb {
        MouseButton::Left => Some(egui::PointerButton::Primary),
        MouseButton::Right => Some(egui::PointerButton::Secondary),
        MouseButton::Middle => Some(egui::PointerButton::Middle),
        MouseButton::Back => Some(egui::PointerButton::Extra1),
        MouseButton::Forward => Some(egui::PointerButton::Extra2),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Input {
    bindings: Bindings,
    current_modifiers: ModifiersState,
    move_inputs: [bool; 4],
    egui_events: Vec<egui::Event>,
    last_cusror_pos: (f64, f64),
}
impl Input {
    pub fn new() -> Result<Self, BindsErr> {
        Ok(Self {
            bindings: Bindings::new()?,
            current_modifiers: ModifiersState::default(),
            move_inputs: [false; 4],
            egui_events: Vec::default(),
            last_cusror_pos: (0.0, 0.0),
        })
    }
    fn handle_bind_out(&mut self, bind_out: BindOut, state: ElementState) -> Option<Action> {
        if let Some(action) = Action::from_bind_out(bind_out) {
            if state.is_pressed() {
                return Some(action);
            }
        } else {
            match bind_out {
                BindOut::MoveForward => self.move_inputs[0] = state.is_pressed(),
                BindOut::MoveLeft => self.move_inputs[1] = state.is_pressed(),
                BindOut::MoveRight => self.move_inputs[2] = state.is_pressed(),
                BindOut::MoveBack => self.move_inputs[3] = state.is_pressed(),
                _ => panic!("Shouldn't happen"),
            }
        }
        None
    }
    pub fn egui_modifiers(&self) -> egui::Modifiers {
        let super_mod = self.current_modifiers.contains(ModifiersState::SUPER);
        let ctrl = self.current_modifiers.contains(ModifiersState::CONTROL);
        egui::Modifiers {
            alt: self.current_modifiers.contains(ModifiersState::ALT),
            ctrl,
            shift: self.current_modifiers.contains(ModifiersState::SHIFT),
            mac_cmd: if cfg!(target_os = "ios") {
                super_mod
            } else {
                false
            },
            command: if cfg!(target_os = "ios") {
                super_mod
            } else {
                ctrl
            },
        }
    }
    pub fn egui_events(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.egui_events)
    }
    pub fn handle_key(&mut self, k: KeyEvent) -> Option<Action> {
        if let Some(key) = match &k.logical_key {
            Key::Named(named) => key_from_named_key(*named),
            Key::Character(str) => egui::Key::from_name(str),
            _ => None,
        } {
            let physical_key = match k.physical_key {
                PhysicalKey::Code(kc) => key_from_key_code(kc),
                _ => None,
            };
            self.egui_events.push(egui::Event::Key {
                key,
                physical_key,
                pressed: k.state.is_pressed(),
                repeat: k.repeat,
                modifiers: self.egui_modifiers(),
            });
        }
        let bind_out_maybe = 'blk: {
            match k.logical_key {
                Key::Named(named) => {
                    if let Some(bind_out) = self.bindings.0.get(&BindType::Named(named)) {
                        break 'blk Some(*bind_out);
                    }
                }
                Key::Character(ch) => {
                    if let Some(bind_out) = self.bindings.0.get(&BindType::Character(ch)) {
                        break 'blk Some(*bind_out);
                    }
                }
                _ => (),
            }
            if let PhysicalKey::Code(kc) = k.physical_key {
                if let Some(bind_out) = self
                    .bindings
                    .0
                    .get(&BindType::KeyMod(kc, self.current_modifiers))
                {
                    break 'blk Some(*bind_out);
                } else if let Some(bind_out) = self.bindings.0.get(&BindType::Key(kc)) {
                    break 'blk Some(*bind_out);
                }
            }
            None
        };
        self.handle_bind_out(bind_out_maybe?, k.state)
    }
    pub fn handle_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        self.last_cusror_pos = (position.x, position.y);
        self.egui_events.push(egui::Event::PointerMoved(Pos2::new(
            position.x as f32,
            position.y as f32,
        )));
    }
    pub fn handle_mouse_button(
        &mut self,
        state: ElementState,
        button: MouseButton,
    ) -> Option<Action> {
        if let Some(pointer_button) = mouse_button_to_pointer_button(button) {
            self.egui_events.push(egui::Event::PointerButton {
                pos: egui::Pos2::new(self.last_cusror_pos.0 as f32, self.last_cusror_pos.1 as f32),
                button: pointer_button,
                pressed: state.is_pressed(),
                modifiers: self.egui_modifiers(),
            });
        }
        if state.is_pressed() {
            if let Some(bind_out) = self.bindings.0.get(&BindType::MouseButton(button)) {
                return self.handle_bind_out(*bind_out, state);
            }
        }
        None
    }
    pub fn handle_mouse_delta(&mut self, delta: (f32, f32)) -> Action {
        self.egui_events
            .push(egui::Event::MouseMoved(egui::Vec2::new(delta.0, delta.1)));
        Action::Look(delta.0 as f32, delta.1 as f32)
    }
    pub fn query_move(&self) -> (f32, f32) {
        let mut move_amt = (0.0, 0.0);

        if self.move_inputs[0] {
            move_amt.1 += 1.0;
        }
        if self.move_inputs[1] {
            move_amt.0 -= 1.0;
        }
        if self.move_inputs[2] {
            move_amt.0 += 1.0;
        }
        if self.move_inputs[3] {
            move_amt.1 -= 1.0;
        }

        if move_amt.0 != 0.0 && move_amt.1 != 0.0 {
            let inv_mag = 1.0 / (move_amt.0 * move_amt.0 + move_amt.1 * move_amt.1);
            move_amt.0 *= inv_mag;
            move_amt.1 *= inv_mag;
        }

        move_amt
    }
}
