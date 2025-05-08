use std::{
    convert::Infallible,
    error::Error,
    fmt::{Debug, Display},
    sync::{
        mpsc::{Receiver, Sender},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};

use log::{log_enabled, Level};
use serde::{Deserialize, Serialize};

use crate::{
    framework::{Component, Components, Comptainer, Entity},
    input::{self, Action, ActionFlags, Input},
    physics::{PhysicsBody, PhysicsProcess},
    render::{Camera, RenderMessage, StaticMeshInstance, UpdateUI},
    System, SystemMessage,
};

use hydrolox_pga3d::prelude as pga;

#[derive(Debug)]
pub enum GameMessage {
    Stop,
    Input(input::Action),
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
    pub fn global_motor(&self, transforms: &Comptainer<Transform>) -> pga::Motor {
        if let Some(parent) = self.parent {
            transforms
                .get(parent)
                .unwrap()
                .global_motor(transforms)
                .combine(self.motor)
        } else {
            self.motor
        }
    }
    pub fn parent_motor(&self, transforms: &Comptainer<Transform>) -> Option<pga::Motor> {
        Some(
            transforms
                .get(self.parent?)
                .unwrap()
                .global_motor(transforms),
        )
    }
}
impl Component for Transform {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHandler {
    handled_actions: ActionFlags,
    action_queue: Vec<Action>,
}
impl ActionHandler {
    pub fn new(handled_actions: ActionFlags) -> Self {
        Self {
            handled_actions,
            action_queue: Vec::default(),
        }
    }
    /// Returns true if the action was taken, false otherwise
    fn try_handle_action(&mut self, action: Action) -> bool {
        if self.handled_actions.contains(action.discriminant()) {
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
    Early,
    Normal,
    Physics,
    Late,
}
impl Default for ProcessStage {
    fn default() -> Self {
        Self::Normal
    }
}

pub trait Process: Send + Debug {
    fn new(components: &Components, render_sender: &Sender<RenderMessage>) -> Self
    where
        Self: Sized;
    fn step(&mut self, components: &Components, input: &RwLock<Input>);
    fn stage(&self) -> ProcessStage {
        ProcessStage::default()
    }
}

pub const STEP_PERIOD: f32 = 1.0 / 60.0;

#[derive(Debug)]
struct Player {
    player_entity: Entity,
    move_speed: f32,
    look_speed: f32,
    look_rot: (f32, f32),
}
impl Process for Player {
    fn new(components: &Components, _: &Sender<RenderMessage>) -> Self
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
            ActionHandler::new(Action::Look(0.0, 0.0).discriminant()),
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
                _ => (), //panic!("Player got unknown action"),
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

pub trait UIElement: Send + Sync + Debug {
    fn active(&self) -> bool;
    fn show(&mut self, ctx: &egui::Context);
}

#[derive(Debug)]
pub struct UIComponent {
    element: Box<dyn UIElement>,
}
impl Component for UIComponent {}

#[derive(Debug)]
struct UI {
    ctx: egui::Context,
    start_time: Instant,
    render_sender: Sender<RenderMessage>,
}
impl Process for UI {
    fn new(_: &Components, render_sender: &Sender<RenderMessage>) -> Self
    where
        Self: Sized,
    {
        Self {
            ctx: egui::Context::default(),
            start_time: Instant::now(),
            render_sender: render_sender.clone(),
        }
    }
    fn stage(&self) -> ProcessStage {
        ProcessStage::Late
    }
    fn step(&mut self, components: &Components, input: &RwLock<Input>) {
        let mut input = input.write().unwrap();
        let raw_input = egui::RawInput {
            time: Some(Instant::now().duration_since(self.start_time).as_secs_f64()),
            predicted_dt: STEP_PERIOD,
            modifiers: input.egui_modifiers(),
            events: input.egui_events(),
            ..Default::default()
        };
        let mut ui_comps = components.uis.write().unwrap();
        let full_output = self.ctx.run(raw_input, |ctx| {
            for (_e, comp) in ui_comps.iter_mut() {
                if comp.element.active() {
                    comp.element.show(ctx);
                }
            }
        });
        let ui_update = UpdateUI {
            textures_delta: full_output.textures_delta,
            primitives: self
                .ctx
                .tessellate(full_output.shapes, full_output.pixels_per_point),
        };
        let _ = self.render_sender.send(RenderMessage::UpdateUI(ui_update));
    }
}

#[derive(Debug)]
struct TestUI {}
impl UIElement for TestUI {
    fn active(&self) -> bool {
        true
    }
    fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("UI Window")
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("It works!");
                if ui.button("Cool Button").clicked() {
                    log::info!("egui button clicked!")
                }
            });
    }
}

#[derive(Debug)]
pub struct Game {
    receiver: Receiver<GameMessage>,
    components: Arc<Components>,
    render_sender: Sender<RenderMessage>,
    input: Arc<RwLock<Input>>,
    early_processes: Vec<Box<dyn Process>>,
    normal_processes: Vec<Box<dyn Process>>,
    physics_processes: Vec<Box<dyn Process>>,
    late_processes: Vec<Box<dyn Process>>,
    paused: bool,
}
impl Game {
    fn add_process<P: Process + 'static>(&mut self) {
        let process = P::new(&self.components, &self.render_sender);
        match process.stage() {
            ProcessStage::Early => self.early_processes.push(Box::new(process)),
            ProcessStage::Normal => self.normal_processes.push(Box::new(process)),
            ProcessStage::Physics => self.physics_processes.push(Box::new(process)),
            ProcessStage::Late => self.late_processes.push(Box::new(process)),
        }
    }
}
impl System for Game {
    type Init = Sender<RenderMessage>;
    type InitErr = Infallible;
    type Err = GameError;
    type Msg = GameMessage;

    fn new(
        components: &Arc<Components>,
        input: &Arc<RwLock<Input>>,
        render_sender: Sender<RenderMessage>,
        receiver: Receiver<GameMessage>,
    ) -> Result<Self, Infallible> {
        let mut me = Self {
            receiver,
            components: components.clone(),
            render_sender: render_sender,
            input: input.clone(),
            early_processes: Vec::default(),
            normal_processes: Vec::default(),
            physics_processes: Vec::default(),
            late_processes: Vec::default(),
            paused: false,
        };

        me.add_process::<PhysicsProcess>();
        me.add_process::<UI>();
        me.add_process::<Player>();

        components.uis.write().unwrap().add_component(
            Entity::new(),
            UIComponent {
                element: Box::new(TestUI {}),
            },
        );

        let make_monkey = |x: f32, y: f32, z: f32| {
            let monkey_entity = Entity::new();

            components.transforms.write().unwrap().add_component(
                monkey_entity,
                Transform::new(None, pga::Motor::from_translation(x, y, z)),
            );
            components
                .static_mesh_instances
                .write()
                .unwrap()
                .add_component(
                    monkey_entity,
                    StaticMeshInstance::new("monkey".into(), "slate_gray".into()),
                );
            let mut phys_body = PhysicsBody::default();
            phys_body.angular_velocity.y = 180.0f32.to_radians();
            components
                .physics_bodies
                .write()
                .unwrap()
                .add_component(monkey_entity, phys_body);
        };

        make_monkey(2.0, 0.0, -2.0);
        make_monkey(0.0, 0.0, -2.0);
        make_monkey(-2.0, 0.0, 0.0);

        Ok(me)
    }
    fn run(&mut self) -> Result<(), GameError> {
        let mut last_loop_start = Instant::now();
        let mut next_time = last_loop_start;
        loop {
            for msg in self.receiver.try_iter() {
                match msg {
                    GameMessage::Stop => return Ok(()),
                    GameMessage::Input(action) => {
                        if !self.paused {
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
            }

            if log_enabled!(Level::Trace) {
                let elapsed = Instant::now().duration_since(last_loop_start).as_secs_f32();
                log::trace!(
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

            if !self.paused {
                self.early_processes
                    .iter_mut()
                    .for_each(|process| process.step(&self.components, &self.input));

                self.normal_processes
                    .iter_mut()
                    .for_each(|process| process.step(&self.components, &self.input));

                self.physics_processes
                    .iter_mut()
                    .for_each(|process| process.step(&self.components, &self.input));

                self.late_processes
                    .iter_mut()
                    .for_each(|process| process.step(&self.components, &self.input));
            }
        }
    }
}
