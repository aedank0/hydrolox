use std::{
    convert::Infallible, error::Error, fmt::Display, sync::mpsc::{channel, Receiver}, thread
};

use serde::{Deserialize, Serialize};
use winit::{
    event::{ElementState, KeyEvent, MouseButton},
};

use crate::{framework::{Component, Entity}, System, SystemData, SystemMessage};

use hydrolox_pga3d as pga;

#[derive(Debug)]
pub enum GameMessage {
    Stop,
    Keyboard(KeyEvent),
    MouseMove((f64, f64)),
    MouseButton((MouseButton, ElementState)),
}
impl SystemMessage for GameMessage {
    fn stop_msg() -> Self {
        Self::Stop
    }
    fn system_name() -> &'static str {
        "Game"
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transform {
    pub parent: Option<Entity>,
    pub transform: pga::transform::Transform,
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

    fn new(_: ()) -> Result<SystemData<GameError, GameMessage>, Infallible> {
        let (sender, receiver) = channel();

        let mut game = Self { receiver };

        Ok(SystemData::new(thread::spawn(move || game.run()), sender))
    }
    fn run(&mut self) -> Result<(), GameError> {
        loop {
            for msg in self.receiver.try_iter() {
                match msg {
                    GameMessage::Stop => return Ok(()),
                    GameMessage::Keyboard(key) => todo!(),
                    GameMessage::MouseMove(amt) => todo!(),
                    GameMessage::MouseButton(mb) => todo!(),
                }
            }
        }
    }
}
