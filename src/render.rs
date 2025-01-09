use std::{
    error::Error,
    fmt::Display,
    mem::size_of,
    sync::{
        mpsc::{channel, Receiver},
        Arc,
    },
    thread,
};

use log::warn;
use smallvec::smallvec;
use vulkano::{
    buffer::BufferContents,
    descriptor_set::layout::{
        DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo,
        DescriptorType,
    },
    device::{
        physical::PhysicalDeviceType, Device, DeviceCreateInfo, Queue, QueueCreateInfo, QueueFlags,
    },
    format::Format,
    image::{ImageAspects, ImageLayout, ImageUsage},
    instance::{Instance, InstanceCreateInfo},
    pipeline::{
        graphics::{
            color_blend::{ColorBlendAttachmentState, ColorBlendState},
            depth_stencil::{CompareOp, DepthState, DepthStencilState},
            input_assembly::InputAssemblyState,
            rasterization::{CullMode, RasterizationState},
            subpass::PipelineSubpassType,
            vertex_input::{
                Vertex, VertexDefinition, VertexInputState,
            },
            viewport::{Viewport, ViewportState},
            GraphicsPipelineCreateInfo,
        },
        layout::{PipelineLayoutCreateInfo, PushConstantRange},
        GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo,
    },
    render_pass::{
        AttachmentDescription, AttachmentLoadOp, AttachmentReference, AttachmentStoreOp,
        RenderPass, RenderPassCreateInfo, Subpass, SubpassDependency, SubpassDescription,
    },
    shader::ShaderStages,
    swapchain::{PresentMode, Surface, SurfaceInfo, Swapchain, SwapchainCreateInfo},
    sync::{AccessFlags, DependencyFlags, PipelineStages},
    LoadingError, Validated, ValidationError, VulkanError, VulkanLibrary,
};
use winit::window::Window;

use crate::{System, SystemData, SystemMessage};

mod shader {
    pub mod fore {
        pub mod vs {
            vulkano_shaders::shader! {
                ty: "vertex",
                path: "src/shader/fore_vs.glsl"
            }
        }
        pub mod fs {
            vulkano_shaders::shader! {
                ty: "fragment",
                path: "src/shader/fore_fs.glsl"
            }
        }
    }
    pub mod defer {
        pub mod vs {
            vulkano_shaders::shader! {
                ty: "vertex",
                path: "src/shader/defer_vs.glsl"
            }
        }
        pub mod fs {
            vulkano_shaders::shader! {
                ty: "fragment",
                path: "src/shader/defer_fs.glsl"
            }
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    LoadErr(LoadingError),
    ValidationErr(Box<ValidationError>),
    VulkanErr(VulkanError),
    NoDevice,
    NoSrfFormats,
    NoGraphicsQueue,
    NoTransferQueue,
    BadShaderEntry,
}
impl Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadErr(err) => writeln!(f, "Failed to load vulkan lib: {err}"),
            Self::ValidationErr(err) => writeln!(f, "Validation failed: {err}"),
            Self::VulkanErr(err) => writeln!(f, "Vulkan error: {err}"),
            Self::NoDevice => writeln!(f, "No device that supports vulkan"),
            Self::NoSrfFormats => writeln!(f, "Device has no supported surface formats"),
            Self::NoGraphicsQueue => writeln!(f, "No valid graphics queue"),
            Self::NoTransferQueue => writeln!(f, "No valid transfer queue"),
            Self::BadShaderEntry => writeln!(f, "Shader has bad entry point name"),
        }
    }
}
impl Error for RenderError {}
impl From<LoadingError> for RenderError {
    fn from(value: LoadingError) -> Self {
        Self::LoadErr(value)
    }
}
impl From<Validated<VulkanError>> for RenderError {
    fn from(value: Validated<VulkanError>) -> Self {
        match value {
            Validated::Error(err) => Self::VulkanErr(err),
            Validated::ValidationError(err) => Self::ValidationErr(err),
        }
    }
}
impl From<VulkanError> for RenderError {
    fn from(value: VulkanError) -> Self {
        Self::VulkanErr(value)
    }
}
impl From<Box<ValidationError>> for RenderError {
    fn from(value: Box<ValidationError>) -> Self {
        Self::ValidationErr(value)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, BufferContents)]
#[repr(C)]
struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Vec3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    const fn from_val(v: f32) -> Self {
        Self { x: v, y: v, z: v }
    }
}

#[derive(Debug, Default, Clone, Copy, BufferContents, Vertex)]
#[repr(C)]
struct VertexData {
    #[format(R32G32B32_SFLOAT)]
    pub pos: Vec3,
    #[format(R32G32B32_SFLOAT)]
    pub norm: Vec3,
    #[format(R16G16B16A16_UINT)]
    pub data: [u16; 4],
}

#[derive(Debug, Default, Clone, Copy, PartialEq, BufferContents)]
#[repr(C)]
struct RGBA {
    pub r: u8,
    pub b: u8,
    pub g: u8,
    pub a: u8,
}
impl RGBA {
    const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, b, g, a }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, BufferContents)]
#[repr(C)]
struct MatData {
    pub color: RGBA,
    pub noise: f32,
    pub data: [f32; 3],
}
impl MatData {
    fn new(color: RGBA, noise: f32) -> Self {
        Self {
            color,
            noise,
            data: [0f32; 3],
        }
    }
}

#[derive(Debug)]
pub enum RenderMessage {
    Stop,
}
impl SystemMessage for RenderMessage {
    fn stop_msg() -> Self {
        Self::Stop
    }
    fn system_name() -> &'static str {
        "Render"
    }
}

pub struct RenderInit {
    pub window: Arc<Window>,
    pub res_x: u16,
    pub res_y: u16,
}

#[derive(Debug)]
pub struct Render {
    receiver: Receiver<RenderMessage>,

    instance: Arc<Instance>,
    device: Arc<Device>,
    graphics_queue: Arc<Queue>,
    transfer_queue: Arc<Queue>,
}
impl System for Render {
    type Init = RenderInit;
    type InitErr = RenderError;
    type Err = RenderError;
    type Msg = RenderMessage;
    fn new(
        RenderInit { window, res_x, res_y }: RenderInit
    ) -> Result<SystemData<RenderError, RenderMessage>, RenderError> {
        let (sender, receiver) = channel();

        let library = VulkanLibrary::new()?;

        let required_extensions = Surface::required_extensions(&*window);

        let instance = Instance::new(library, {
            let mut info = InstanceCreateInfo::application_from_cargo_toml();
            info.engine_name = info.application_name.clone();
            info.engine_version = info.application_version.clone();
            info.enabled_extensions = required_extensions;
            info
        })?;

        let surface = Surface::from_window(instance.clone(), window.clone())?;

        let phys_device = instance
            .enumerate_physical_devices()?
            .find(|pd| pd.properties().device_type == PhysicalDeviceType::DiscreteGpu)
            .ok_or(RenderError::NoDevice)?;

        let &(srf_fmt, srf_color_space) = phys_device
            .surface_formats(
                &surface,
                SurfaceInfo {
                    present_mode: Some(PresentMode::Mailbox),
                    ..Default::default()
                },
            )?
            .first()
            .ok_or(RenderError::NoSrfFormats)?;

        let graphics_queue_fam_id = phys_device.queue_family_properties().iter().enumerate().find_map(|(id, fam)| {
            if fam.queue_flags.contains(QueueFlags::GRAPHICS) && match phys_device.surface_support(id as u32, &surface) {
                Ok(v) => v,
                Err(err) => { warn!("Error attempting to query phys device queue surface support: {err}"); false }
            } {
                Some(id as u32)
            } else {
                None
            }
        }).ok_or(RenderError::NoGraphicsQueue)?;
        let transfer_queue_fam_id = phys_device
            .queue_family_properties()
            .iter()
            .enumerate()
            .find_map(|(id, fam)| {
                if fam.queue_flags.contains(QueueFlags::TRANSFER) {
                    Some(id as u32)
                } else {
                    None
                }
            })
            .ok_or(RenderError::NoTransferQueue)?;

        let (device, mut queue_iter) = Device::new(
            phys_device.clone(),
            DeviceCreateInfo {
                queue_create_infos: vec![
                    QueueCreateInfo {
                        queue_family_index: graphics_queue_fam_id,
                        ..Default::default()
                    },
                    QueueCreateInfo {
                        queue_family_index: transfer_queue_fam_id,
                        ..Default::default()
                    },
                ],
                physical_devices: smallvec![],
                ..Default::default()
            },
        )?;
        let graphics_queue = queue_iter.next().unwrap();
        let transfer_queue = queue_iter.next().unwrap();

        let (swapchain, swap_imgs) = Swapchain::new(
            device.clone(),
            surface.clone(),
            SwapchainCreateInfo {
                min_image_count: 3,
                image_format: srf_fmt,
                image_extent: [res_x as u32, res_y as u32],
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                present_mode: PresentMode::Mailbox,
                ..Default::default()
            },
        )?;

        let mat_arr_desc_set_layout = DescriptorSetLayout::new(
            device.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: [(
                    0,
                    DescriptorSetLayoutBinding {
                        stages: ShaderStages::FRAGMENT,
                        ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::UniformBuffer)
                    },
                )]
                .into(),
                ..Default::default()
            },
        )?;

        let fore_pipeline_layout = PipelineLayout::new(
            device.clone(),
            PipelineLayoutCreateInfo {
                set_layouts: vec![mat_arr_desc_set_layout],
                push_constant_ranges: vec![PushConstantRange {
                    stages: ShaderStages::VERTEX,
                    offset: 0,
                    size: size_of::<shader::fore::vs::PushData>() as u32,
                }],
                ..Default::default()
            },
        )?;

        let fore_vs_entry = shader::fore::vs::load(device.clone())?
            .single_entry_point()
            .ok_or(RenderError::BadShaderEntry)?;
        let fore_fs_entry = shader::fore::fs::load(device.clone())?
            .single_entry_point()
            .ok_or(RenderError::BadShaderEntry)?;

        let vert_input_interface = fore_vs_entry.info().input_interface.clone();

        let render_pass = RenderPass::new(
            device.clone(),
            RenderPassCreateInfo {
                attachments: vec![
                    // opaque color
                    AttachmentDescription {
                        format: Format::R8G8B8A8_UNORM,
                        load_op: AttachmentLoadOp::Clear,
                        final_layout: ImageLayout::ShaderReadOnlyOptimal,
                        ..Default::default()
                    },
                    // opaque pos
                    AttachmentDescription {
                        format: Format::R32G32_SFLOAT,
                        load_op: AttachmentLoadOp::Clear,
                        final_layout: ImageLayout::ShaderReadOnlyOptimal,
                        ..Default::default()
                    },
                    // opaque norm
                    AttachmentDescription {
                        format: Format::R16G16_SFLOAT,
                        load_op: AttachmentLoadOp::Clear,
                        final_layout: ImageLayout::ShaderReadOnlyOptimal,
                        ..Default::default()
                    },
                    // depth
                    AttachmentDescription {
                        format: Format::D16_UNORM,
                        load_op: AttachmentLoadOp::Clear,
                        final_layout: ImageLayout::ShaderReadOnlyOptimal,
                        ..Default::default()
                    },
                    // color out
                    AttachmentDescription {
                        format: srf_fmt,
                        load_op: AttachmentLoadOp::Clear,
                        store_op: AttachmentStoreOp::Store,
                        final_layout: ImageLayout::PresentSrc,
                        ..Default::default()
                    },
                ],
                subpasses: vec![
                    // opaque fore
                    SubpassDescription {
                        color_attachments: vec![
                            Some(AttachmentReference {
                                attachment: 0,
                                layout: ImageLayout::ColorAttachmentOptimal,
                                ..Default::default()
                            }),
                            Some(AttachmentReference {
                                attachment: 1,
                                layout: ImageLayout::ColorAttachmentOptimal,
                                ..Default::default()
                            }),
                            Some(AttachmentReference {
                                attachment: 2,
                                layout: ImageLayout::ColorAttachmentOptimal,
                                ..Default::default()
                            }),
                        ],
                        depth_stencil_attachment: Some(AttachmentReference {
                            attachment: 3,
                            layout: ImageLayout::DepthAttachmentOptimal,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // defer
                    SubpassDescription {
                        input_attachments: vec![
                            Some(AttachmentReference {
                                attachment: 0,
                                layout: ImageLayout::ShaderReadOnlyOptimal,
                                aspects: ImageAspects::COLOR,
                                ..Default::default()
                            }),
                            Some(AttachmentReference {
                                attachment: 1,
                                layout: ImageLayout::ShaderReadOnlyOptimal,
                                aspects: ImageAspects::COLOR,
                                ..Default::default()
                            }),
                            Some(AttachmentReference {
                                attachment: 2,
                                layout: ImageLayout::ShaderReadOnlyOptimal,
                                aspects: ImageAspects::COLOR,
                                ..Default::default()
                            }),
                            Some(AttachmentReference {
                                attachment: 3,
                                layout: ImageLayout::DepthReadOnlyOptimal,
                                aspects: ImageAspects::DEPTH,
                                ..Default::default()
                            }),
                        ],
                        color_attachments: vec![Some(AttachmentReference {
                            attachment: 4,
                            layout: ImageLayout::ColorAttachmentOptimal,
                            ..Default::default()
                        })],
                        ..Default::default()
                    },
                ],
                dependencies: vec![SubpassDependency {
                    src_subpass: Some(0),
                    dst_subpass: Some(1),
                    src_stages: PipelineStages::COLOR_ATTACHMENT_OUTPUT
                        | PipelineStages::LATE_FRAGMENT_TESTS,
                    dst_stages: PipelineStages::FRAGMENT_SHADER,
                    src_access: AccessFlags::COLOR_ATTACHMENT_WRITE
                        | AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    dst_access: AccessFlags::INPUT_ATTACHMENT_READ,
                    dependency_flags: DependencyFlags::BY_REGION,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )?;

        let fore_pipeline = GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(fore_vs_entry),
                    PipelineShaderStageCreateInfo::new(fore_fs_entry),
                ],
                vertex_input_state: Some(
                    [VertexData::per_vertex()].definition(&vert_input_interface)?,
                ),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState {
                    viewports: smallvec![Viewport {
                        extent: [res_x as f32, res_y as f32],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                rasterization_state: Some(RasterizationState {
                    cull_mode: CullMode::Back,
                    ..Default::default()
                }),
                depth_stencil_state: Some(DepthStencilState {
                    depth: Some(DepthState {
                        write_enable: true,
                        compare_op: CompareOp::Greater,
                    }),
                    ..Default::default()
                }),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    3,
                    ColorBlendAttachmentState::default(),
                )),
                subpass: Some(PipelineSubpassType::BeginRenderPass(
                    Subpass::from(render_pass.clone(), 0).unwrap(),
                )),
                ..GraphicsPipelineCreateInfo::layout(fore_pipeline_layout)
            },
        )?;

        let input_attach_desc_set = DescriptorSetLayout::new(
            device.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: [
                    //color
                    (
                        0,
                        DescriptorSetLayoutBinding {
                            stages: ShaderStages::FRAGMENT,
                            ..DescriptorSetLayoutBinding::descriptor_type(
                                DescriptorType::InputAttachment,
                            )
                        },
                    ),
                    //norm
                    (
                        1,
                        DescriptorSetLayoutBinding {
                            stages: ShaderStages::FRAGMENT,
                            ..DescriptorSetLayoutBinding::descriptor_type(
                                DescriptorType::InputAttachment,
                            )
                        },
                    ),
                    //pos
                    (
                        2,
                        DescriptorSetLayoutBinding {
                            stages: ShaderStages::FRAGMENT,
                            ..DescriptorSetLayoutBinding::descriptor_type(
                                DescriptorType::InputAttachment,
                            )
                        },
                    ),
                    //depth
                    (
                        3,
                        DescriptorSetLayoutBinding {
                            stages: ShaderStages::FRAGMENT,
                            ..DescriptorSetLayoutBinding::descriptor_type(
                                DescriptorType::InputAttachment,
                            )
                        },
                    ),
                ]
                .into(),
                ..Default::default()
            },
        )?;

        let defer_vs_entry = shader::defer::vs::load(device.clone())?
            .single_entry_point()
            .ok_or(RenderError::BadShaderEntry)?;
        let defer_fs_entry = shader::defer::fs::load(device.clone())?
            .single_entry_point()
            .ok_or(RenderError::BadShaderEntry)?;

        let defer_pipeline_layout = PipelineLayout::new(
            device.clone(),
            PipelineLayoutCreateInfo {
                set_layouts: vec![input_attach_desc_set],
                ..Default::default()
            },
        )?;

        let defer_pipeline = GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(defer_vs_entry),
                    PipelineShaderStageCreateInfo::new(defer_fs_entry),
                ],
                vertex_input_state: Some(VertexInputState::new()),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState {
                    viewports: smallvec![Viewport {
                        extent: [res_x as f32, res_y as f32],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                rasterization_state: Some(RasterizationState {
                    cull_mode: CullMode::Back,
                    ..Default::default()
                }),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    1,
                    ColorBlendAttachmentState::default(),
                )),
                subpass: Some(PipelineSubpassType::BeginRenderPass(
                    Subpass::from(render_pass.clone(), 1).unwrap(),
                )),
                ..GraphicsPipelineCreateInfo::layout(defer_pipeline_layout)
            },
        )?;

        let mut render = Self {
            receiver,
            instance,
            device,
            graphics_queue,
            transfer_queue,
        };

        Ok(SystemData::new(thread::spawn(move || render.run()), sender))
    }
    fn run(&mut self) -> Result<(), RenderError> {
        loop {
            for msg in self.receiver.try_iter() {
                match msg {
                    RenderMessage::Stop => return Ok(()),
                }
            }
        }
    }
}
