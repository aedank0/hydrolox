# Hydrolox

This is a game engine that I've been working on in my spare time over the past year or so. Writing a game engine can be uniquely challenging, as it requires running computationally intensive code in real time and combines many different domains of programming, so I thought it would be a fun thing to dive into. Long-term I'd like to create a game with it, (probably space themed, hence the name: a popular fuel/oxidizer combination for rockets), but the engine itself needs more work before then.

Hydrolox is written in Rust with shaders written in GLSL, and uses the [Vulkano](https://github.com/vulkano-rs/vulkano) library in its renderer, which is a Rust wrapper around the Vulkan API. It also uses the [winit](https://github.com/rust-windowing/winit) library for cross-platform window creation and receiving user input. Each core system (i.e. Input, Game, Physics, Render) runs in its own thread, and use message queues to communicate with each other. This helps to better utilize the cpu's resources as well as reduce spaghettification between the systems.

In place of a linear algebra library, I'm trying to code an implementation of 3D Projective Geometric Algebra, which ideally does fewer multiplications than using a 4x4 matrix. It's in a seperate repository: <https://github.com/aedank0/hydrolox-pga3d>.

Some priorities for what I want to do next:
 - Complete and debug rendering system
 - ~~Refactor the CompData struct to no longer use unsafe code~~
   - ~~Doing some research of other type-erased vec libraries like [any_vec](https://github.com/tower120/any_vec), it seems that using unsafe code is better overall in terms of efficiency and readability~~
   - ~~Instead of factoring out unsafe code, I refactored CompData to have more idiomatic use of NonNull and implemented lazy allocation~~
   - Implementing certain things like deserialization would be significantly more difficult while using type erasure, so I instead decided to just use typed containers, possibly with macros in the future if needed to reduce repetition
 - Finish implementing Game system
   - This uses an ECS based framework for handling game object data and behaviours
   - Each of the ECS systems (which I'll call processes to differentiate from the core systems) will run in parellel using a threadpool
 - Implement the Input system
   - This system will handle user input events and translate them to actions, which will then be sent to the Game system
