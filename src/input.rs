use std::{error::Error, fmt::Display};

use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use serde_yml as yml;
use winit::{
    event::{ElementState, KeyEvent, MouseButton},
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey, SmolStr},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u16)]
pub enum Action {
    PrimaryInteract,
    SecondaryInteract,
    Pause,
    Look(f32, f32),
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
    pub fn discriminant(&self) -> u16 {
        //Safe according to https://doc.rust-lang.org/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        unsafe { *<*const _>::from(self).cast::<u16>() }
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

#[derive(Debug)]
pub struct Input {
    bindings: Bindings,
    current_modifiers: ModifiersState,
    move_inputs: [bool; 4],
}
impl Input {
    pub fn new() -> Result<Self, BindsErr> {
        Ok(Self {
            bindings: Bindings::new()?,
            current_modifiers: ModifiersState::default(),
            move_inputs: [false; 4],
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
    pub fn handle_key(&mut self, k: KeyEvent) -> Option<Action> {
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
        if let Some(bind_out) = bind_out_maybe {
            return self.handle_bind_out(bind_out, k.state);
        }
        None
    }
    pub fn handle_mouse_button(
        &mut self,
        state: ElementState,
        button: MouseButton,
    ) -> Option<Action> {
        if state.is_pressed() {
            if let Some(bind_out) = self.bindings.0.get(&BindType::MouseButton(button)) {
                return self.handle_bind_out(*bind_out, state);
            }
        }
        None
    }
    pub fn handle_mouse_delta(&mut self, delta: (f32, f32)) -> Action {
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
