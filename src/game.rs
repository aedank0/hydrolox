use std::{
    convert::Infallible,
    error::Error,
    fmt::{Debug, Display},
    sync::{mpsc::Receiver, Arc, RwLock},
    time::{Duration, Instant},
};

use ahash::AHashSet;
use log::{log_enabled, Level};
use serde::{Deserialize, Serialize};

use crate::{
    framework::{Component, Components, Entity},
    input::{self, Action, Input},
    render::{Camera, StaticMeshInstance},
    System, SystemMessage,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHandler {
    handled_actions: AHashSet<u16>,
    action_queue: Vec<Action>,
}
impl ActionHandler {
    pub fn new(handled_actions: AHashSet<u16>) -> Self {
        Self {
            handled_actions,
            action_queue: Vec::default(),
        }
    }
    //Returns true is the action was taken, false otherwise
    fn try_handle_action(&mut self, action: Action) -> bool {
        if self.handled_actions.contains(&action.discriminant()) {
            self.action_queue.push(action);
            true
        } else {
            false
        }
    }
}
impl Component for ActionHandler {}

#[derive(Debug)]
pub enum GameError {}
impl Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Game error")
    }
}
impl Error for GameError {}

pub enum ProcessStage {
    Normal,
    Early,
    Late,
}
impl Default for ProcessStage {
    fn default() -> Self {
        Self::Normal
    }
}

pub trait Process: Send + Debug {
    fn new(components: &Components) -> Self
    where
        Self: Sized;
    fn step(&mut self, components: &Components, input: &RwLock<Input>);
    fn stage() -> ProcessStage
    where
        Self: Sized,
    {
        ProcessStage::default()
    }
}

const STEP_PERIOD: f32 = 1.0 / 60.0;

#[derive(Debug)]
struct SpinMonkey {
    monkey_entity: Entity,
    spin_speed: f32,
}
impl Process for SpinMonkey {
    fn new(components: &Components) -> Self
    where
        Self: Sized,
    {
        let monkey_entity = Entity::new();

        components.transforms.write().unwrap().add_component(
            monkey_entity,
            Transform::new(None, pga::Motor::from_translation(1.0, 0.0, -1.0)),
        );
        components
            .static_mesh_instances
            .write()
            .unwrap()
            .add_component(
                monkey_entity,
                StaticMeshInstance::new("monkey".into(), "slate_gray".into()),
            );

        Self {
            monkey_entity,
            spin_speed: 180.0f32.to_radians(),
        }
    }
    fn step(&mut self, components: &Components, _: &RwLock<Input>) {
        let mut motor = components
            .transforms
            .read()
            .unwrap()
            .get(self.monkey_entity)
            .unwrap()
            .motor;
        motor = pga::Motor::from_rotation_around_axis(0.0, 1.0, 0.0, self.spin_speed * STEP_PERIOD)
            .combine(motor);
        components
            .transforms
            .write()
            .unwrap()
            .get_mut(self.monkey_entity)
            .unwrap()
            .motor = motor;
    }
}

#[derive(Debug)]
struct Player {
    player_entity: Entity,
    move_speed: f32,
    look_speed: f32,
    look_rot: (f32, f32),
}
impl Process for Player {
    fn new(components: &Components) -> Self
    where
        Self: Sized,
    {
        let player_entity = Entity::new();
        components
            .transforms
            .write()
            .unwrap()
            .add_component(player_entity, Transform::new(None, pga::Motor::IDENTITY));
        components.cameras.write().unwrap().add_component(
            player_entity,
            Camera {
                fov: 45.0,
                near_plane: 0.1,
            },
        );
        components.action_handlers.write().unwrap().add_component(
            player_entity,
            ActionHandler::new([Action::Look(0.0, 0.0).discriminant()].into()),
        );

        Self {
            player_entity,
            move_speed: 3.0,
            look_speed: 0.3,
            look_rot: (0.0, 0.0),
        }
    }
    fn step(&mut self, components: &Components, input: &RwLock<Input>) {
        let actions = std::mem::take(
            &mut components
                .action_handlers
                .write()
                .unwrap()
                .get_mut(self.player_entity)
                .unwrap()
                .action_queue,
        );

        for action in actions {
            match action {
                Action::Look(x, y) => {
                    self.look_rot.0 += x * self.look_speed * STEP_PERIOD;
                    self.look_rot.1 += y * self.look_speed * STEP_PERIOD;
                    self.look_rot.1 = self
                        .look_rot
                        .1
                        .clamp((-80.0f32).to_radians(), (80.0f32).to_radians());

                    let mut transforms = components.transforms.write().unwrap();
                    let motor = &mut transforms.get_mut(self.player_entity).unwrap().motor;

                    let translation = motor.factor_translation();
                    let rotation =
                        pga::Motor::from_euler_angles(-self.look_rot.1, -self.look_rot.0, 0.0);
                    *motor = rotation.combine(translation);
                }
                _ => panic!("Player got unknown action"),
            }
        }

        let move_amt = input.read().unwrap().query_move();
        if move_amt.0 != 0.0 || move_amt.1 != 0.0 {
            let mut transforms = components.transforms.write().unwrap();
            let motor = &mut transforms.get_mut(self.player_entity).unwrap().motor;

            let to_move = motor.factor_rotation().transform(pga::Point::from_position(
                move_amt.0 * self.move_speed * STEP_PERIOD,
                0.0,
                -move_amt.1 * self.move_speed * STEP_PERIOD,
            ));
            *motor = motor.combine(pga::Motor::from_translation(
                to_move.x, to_move.y, to_move.z,
            ));
        }
    }
}

#[derive(Debug)]
pub struct Game {
    receiver: Receiver<GameMessage>,
    components: Arc<Components>,
    input: Arc<RwLock<Input>>,
    early_processes: Vec<Box<dyn Process>>,
    normal_processes: Vec<Box<dyn Process>>,
    late_processes: Vec<Box<dyn Process>>,
}
impl Game {
    fn add_process<P: Process + 'static>(&mut self) {
        let process = P::new(&self.components);
        match P::stage() {
            ProcessStage::Normal => self.normal_processes.push(Box::new(process)),
            ProcessStage::Early => self.early_processes.push(Box::new(process)),
            ProcessStage::Late => self.late_processes.push(Box::new(process)),
        }
    }
}
impl System for Game {
    type Init = ();
    type InitErr = Infallible;
    type Err = GameError;
    type Msg = GameMessage;

    fn new(
        components: &Arc<Components>,
        input: &Arc<RwLock<Input>>,
        _: (),
        receiver: Receiver<GameMessage>,
    ) -> Result<Self, Infallible> {
        let mut me = Self {
            receiver,
            components: components.clone(),
            input: input.clone(),
            early_processes: Vec::default(),
            normal_processes: Vec::default(),
            late_processes: Vec::default(),
        };

        me.add_process::<SpinMonkey>();
        me.add_process::<Player>();

        Ok(me)
    }
    fn run(&mut self) -> Result<(), GameError> {
        let mut last_loop_start = Instant::now();
        let mut next_time = last_loop_start;
        loop {
            for msg in self.receiver.try_iter() {
                match msg {
                    GameMessage::Stop => return Ok(()),
                    GameMessage::Action(action) => {
                        for (_, handler) in
                            self.components.action_handlers.write().unwrap().iter_mut()
                        {
                            if handler.try_handle_action(action) {
                                break;
                            }
                        }
                    }
                }
            }

            if log_enabled!(Level::Info) {
                let elapsed = Instant::now().duration_since(last_loop_start).as_secs_f32();
                log::info!(
                    "Game loop took: {}ms, {}% work",
                    elapsed * 1000.0,
                    elapsed / STEP_PERIOD * 100.0
                );
            }
            if let Some(to_sleep) = next_time.checked_duration_since(Instant::now()) {
                std::thread::sleep(to_sleep);
            }
            last_loop_start = Instant::now();
            next_time = last_loop_start + Duration::from_secs_f32(STEP_PERIOD);

            self.early_processes
                .iter_mut()
                .for_each(|process| process.step(&self.components, &self.input));

            self.normal_processes
                .iter_mut()
                .for_each(|process| process.step(&self.components, &self.input));

            self.late_processes
                .iter_mut()
                .for_each(|process| process.step(&self.components, &self.input));
        }
    }
}
