use std::{
    convert::Infallible,
    error::Error,
    fmt::Display,
    sync::{mpsc::Receiver, Arc},
};

use serde::{Deserialize, Serialize};
use winit::event::{ElementState, KeyEvent, MouseButton};

use crate::{
    framework::{Component, Components, Entity}, input, System, SystemMessage
};

use hydrolox_pga3d::prelude as pga;

#[derive(Debug)]
pub enum GameMessage {
    Stop,
    Action(input::Action),
}
impl SystemMessage for GameMessage {
    fn stop_msg() -> Self {
        Self::Stop
    }
    fn system_name() -> &'static str {
        "Game"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub parent: Option<Entity>,
    pub motor: pga::Motor,
}
impl Transform {
    pub fn new(parent: Option<Entity>, motor: pga::Motor) -> Self {
        Self { parent, motor }
    }
    pub fn global_motor(&self, comps: &Components) -> pga::Motor {
        if let Some(parent) = self.parent {
            comps
                .transforms
                .read()
                .unwrap()
                .get(parent)
                .unwrap()
                .global_motor(comps)
                .combine(self.motor)
        } else {
            self.motor
        }
    }
}
impl Component for Transform {}

#[derive(Debug)]
pub enum GameError {}
impl Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Game error")
    }
}
impl Error for GameError {}

#[derive(Debug)]
pub struct Game {
    receiver: Receiver<GameMessage>,
}
impl System for Game {
    type Init = ();
    type InitErr = Infallible;
    type Err = GameError;
    type Msg = GameMessage;

    fn new(
        _: &Arc<Components>,
        _: (),
        receiver: Receiver<GameMessage>,
    ) -> Result<Self, Infallible> {
        Ok(Self { receiver })
    }
    fn run(&mut self) -> Result<(), GameError> {
        loop {
            for msg in self.receiver.try_iter() {
                match msg {
                    GameMessage::Stop => return Ok(()),
                    GameMessage::Action(action) => todo!(),
                }
            }
        }
    }
}
