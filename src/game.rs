use std::{
    any::Any, error::Error, fmt::Display, sync::mpsc::{channel, Receiver, Sender}, thread
};

use winit::{
    event::{ElementState, KeyEvent, MouseButton},
    keyboard::KeyCode,
};

use crate::{SystemData, SystemMessage};

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
impl Game {
    pub fn new() -> SystemData<GameError, GameMessage> {
        let (sender, receiver) = channel();

        let mut game = Self { receiver };

        SystemData::new(thread::spawn(move || game.run()), sender)
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