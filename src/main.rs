use std::{
    error::Error,
    panic,
    process::ExitCode,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, RwLock,
    },
    thread::{self, JoinHandle},
};

use framework::Components;
use game::{Game, GameError, GameMessage};
use input::Input;
use log::{error, info};
use render::{Render, RenderError, RenderInit, RenderMessage};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalSize, Size},
    event::{DeviceEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

//mod comp_data;
mod framework;
mod game;
mod render;
mod input;

pub trait SystemMessage {
    fn stop_msg() -> Self;
    fn system_name() -> &'static str;
}

#[derive(Debug)]
pub struct SystemData<E: Error, M: SystemMessage> {
    thread: Option<JoinHandle<Result<(), E>>>,
    sender: Sender<M>,
}
impl<E: Error, M: SystemMessage> SystemData<E, M> {
    pub fn new(thread: JoinHandle<Result<(), E>>, sender: Sender<M>) -> Self {
        Self {
            thread: Some(thread),
            sender,
        }
    }
    fn is_thread_active(&self) -> bool {
        !self.thread.as_ref().unwrap().is_finished()
    }
}
impl<E: Error, M: SystemMessage> Drop for SystemData<E, M> {
    fn drop(&mut self) {
        if let Err(err) = self.sender.send(M::stop_msg()) {
            error!("Failed to send stop to {}: {err}", M::system_name());
        }
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(res) => {
                    if let Err(err) = res {
                        error!("{} thread returned with an error: {err}", M::system_name());
                    }
                }
                Err(err) => {
                    if let Ok(err_str) = err.downcast::<&str>() {
                        error!("{} thread finished with panic: {err_str}", M::system_name());
                    } else {
                        error!(
                            "{} thread finished with panic of unknown type",
                            M::system_name()
                        );
                    }
                }
            }
        }
    }
}

pub trait System: Send + 'static {
    type Init;
    type InitErr: Error;
    type Err: Error + Send;
    type Msg: SystemMessage;

    fn new(
        comps: &Arc<Components>,
        init: Self::Init,
        recv: Receiver<Self::Msg>,
    ) -> Result<Self, Self::InitErr>
    where
        Self: Sized;
    fn run(&mut self) -> Result<(), Self::Err>;
}

fn new_system<T: System>(
    comps: &Arc<Components>,
    init: T::Init,
) -> Result<SystemData<T::Err, T::Msg>, T::InitErr> {
    let (sender, receiver) = channel();

    let mut sys = T::new(comps, init, receiver)?;

    Ok(SystemData::new(thread::spawn(move || sys.run()), sender))
}

#[derive(Debug)]
struct App {
    window: Option<Arc<Window>>,
    render: Option<SystemData<RenderError, RenderMessage>>,
    game: Option<SystemData<GameError, GameMessage>>,
    components: Arc<Components>,
    input: Arc<RwLock<Input>>,
}
impl App {
    fn new() -> Self {
        let components = Arc::default();
        let input = Arc::new(RwLock::new(match Input::new() {
            Ok(i) => i,
            Err(err) => {
                log::error!("Failed to initialize bindings: {err}");
                panic!();
            }
        }));

        Self { window: None, render: None, game: None, components, input }
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            self.window = Some(
                match event_loop.create_window(
                    Window::default_attributes()
                        .with_inner_size(Size::Physical(PhysicalSize::new(1280, 720))),
                ) {
                    Ok(w) => Arc::new(w),
                    Err(err) => {
                        error!("OS failed to create window: {err}");
                        panic!();
                    }
                },
            );
        }
        if self.render.is_none() {
            self.render = match new_system::<Render>(
                &self.components,
                RenderInit {
                    window: self.window.as_ref().unwrap().clone(),
                    res_x: 1280,
                    res_y: 720,
                },
            ) {
                Ok(r) => Some(r),
                Err(err) => {
                    error!("Failed to init rendering: {err}");
                    panic!();
                }
            };
        }
        if self.game.is_none() {
            self.game = Some(new_system::<Game>(&self.components, ()).unwrap());
        }
    }
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(), //Possibly do something else here first (ex saving, asking if sure)
            WindowEvent::KeyboardInput {
                device_id: _,
                event: key,
                is_synthetic: _,
            } => {
                let action_maybe = self.input.write().unwrap().handle_key(key);
                if let Some(action) = action_maybe {
                    if let Err(err) = self
                        .game
                        .as_ref()
                        .unwrap()
                        .sender
                        .send(GameMessage::Action(action))
                    {
                        error!("Failed to send key event to game: {err}");
                        panic!();
                    }
                }
            }
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button,
            } => {
                let action_maybe = self.input.write().unwrap().handle_mouse_button(state, button);
                if let Some(action) = action_maybe {
                    if let Err(err) = self
                        .game
                        .as_ref()
                        .unwrap()
                        .sender
                        .send(GameMessage::Action(action))
                    {
                        error!("Failed to send mouse button event to game: {err}");
                        panic!();
                    }
                }
            }
            _ => (),
        }
    }
    fn device_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        match event {
            DeviceEvent::MouseMotion { delta: (x, y) } => {
                let action = self.input.write().unwrap().handle_mouse_delta((x as f32, y as f32));
                if let Err(err) = self
                    .game
                    .as_ref()
                    .unwrap()
                    .sender
                    .send(GameMessage::Action(action))
                {
                    error!("Failed to send mouse motion to game: {err}");
                    panic!();
                }
            }
            _ => (),
        }
    }
    fn new_events(&mut self, _: &winit::event_loop::ActiveEventLoop, _: winit::event::StartCause) {
        if !self.render.as_ref().unwrap().is_thread_active() {
            let mut render = self.render.take().unwrap();
            match render.thread.take().unwrap().join() {
                Ok(res) => {
                    if let Err(err) = res {
                        error!("Render error: {err}");
                    } else {
                        error!("Renderer stopped without error");
                    }
                    panic!();
                }
                Err(payload) => {
                    error!("Render thread panicked");
                    panic::resume_unwind(payload);
                }
            }
        }
        if !self.game.as_ref().unwrap().is_thread_active() {
            let mut game = self.game.take().unwrap();
            if let Err(payload) = game.thread.take().unwrap().join() {
                error!("Game thread panicked");
                panic::resume_unwind(payload);
            } else {
                error!("Game stopped without error");
                panic!();
            }
        }
    }
    fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {
        //Something maybe??????
        //drop(self.game.take().unwrap());
        //drop(self.render.take().unwrap());
    }
}

fn main() -> ExitCode {
    env_logger::init();

    info!("Starting Hydrolox");
    info!("Hello World!");

    let event_loop = match EventLoop::new() {
        Ok(ev) => ev,
        Err(err) => {
            error!("Error when creating event loop: {err}");
            return ExitCode::FAILURE;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new();
    if let Err(err) = event_loop.run_app(&mut app) {
        error!("Error running event loop: {err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
