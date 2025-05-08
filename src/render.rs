use std::{
    collections::hash_map::Entry,
    error::Error,
    fmt::{Debug, Display},
    fs::File,
    mem::{offset_of, size_of},
    num::{NonZeroU16, NonZeroU64},
    sync::{mpsc::Receiver, Arc, RwLock}, time::{Duration, Instant},
};

use ahash::AHashMap;
use bytemuck::{Pod, Zeroable};
use egui::TextureId;
use hydrolox_pga3d::prelude as pga;
use log::warn;
use serde::{Deserialize, Serialize};
use serde_yml as yml;
use smallvec::{smallvec, SmallVec};
use vulkano::{
    buffer::{
        AllocateBufferError, Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer,
    },
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, BufferImageCopy, CommandBufferExecError, CommandBufferExecFuture,
        CommandBufferUsage, CopyBufferInfo, CopyBufferInfoTyped, CopyBufferToImageInfo,
        PrimaryCommandBufferAbstract, RenderPassBeginInfo, SubpassBeginInfo, SubpassEndInfo,
    },
    descriptor_set::{
        allocator::{StandardDescriptorSetAllocator, StandardDescriptorSetAllocatorCreateInfo},
        layout::{
            DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo,
            DescriptorType,
        },
        DescriptorSet, WriteDescriptorSet,
    },
    device::{
        physical::PhysicalDeviceType, Device, DeviceCreateInfo, DeviceExtensions, Queue,
        QueueCreateInfo, QueueFlags,
    },
    format::{ClearValue, Format},
    image::{
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo, SamplerMipmapMode},
        view::ImageView,
        AllocateImageError, Image, ImageAspects, ImageCreateInfo, ImageLayout,
        ImageSubresourceLayers, ImageTiling, ImageType, ImageUsage,
    },
    instance::{Instance, InstanceCreateInfo},
    memory::allocator::{
        AllocationCreateInfo, BumpAllocator, GenericMemoryAllocator,
        GenericMemoryAllocatorCreateInfo, MemoryTypeFilter, StandardMemoryAllocator,
    },
    pipeline::{
        graphics::{
            color_blend::{
                AttachmentBlend, BlendFactor, ColorBlendAttachmentState, ColorBlendState,
            },
            depth_stencil::{CompareOp, DepthState, DepthStencilState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::{CullMode, RasterizationState},
            subpass::PipelineSubpassType,
            vertex_input::{Vertex, VertexDefinition, VertexInputState},
            viewport::{Scissor, Viewport, ViewportState},
            GraphicsPipelineCreateInfo,
        },
        layout::{PipelineLayoutCreateInfo, PushConstantRange},
        DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout,
        PipelineShaderStageCreateInfo,
    },
    render_pass::{
        AttachmentDescription, AttachmentLoadOp, AttachmentReference, AttachmentStoreOp,
        Framebuffer, FramebufferCreateInfo, RenderPass, RenderPassCreateInfo, Subpass,
        SubpassDependency, SubpassDescription,
    },
    shader::ShaderStages,
    swapchain::{
        acquire_next_image, FromWindowError, PresentFuture, PresentMode, Surface, SurfaceInfo,
        Swapchain, SwapchainCreateInfo, SwapchainPresentInfo,
    },
    sync::{
        future::FenceSignalFuture, AccessFlags, DependencyFlags, GpuFuture, PipelineStages, Sharing,
    },
    LoadingError, Validated, ValidationError, VulkanError, VulkanLibrary,
};
use winit::{raw_window_handle::HandleError, window::Window};

use crate::{
    framework::{Component, Components, Entity}, geometry::*, input::Input, timer::Timer, System, SystemMessage
};

mod shader {
    pub mod fore {
        pub mod vs {
            vulkano_shaders::shader! {
                ty: "vertex",
                path: "src/shader/fore_vs.glsl",
                custom_derives: [Debug],
            }
        }
        pub mod fs {
            vulkano_shaders::shader! {
                ty: "fragment",
                path: "src/shader/fore_fs.glsl",
                custom_derives: [Debug, serde::Serialize, serde::Deserialize],
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
    pub mod ui {
        pub mod vs {
            vulkano_shaders::shader! {
                ty: "vertex",
                path: "src/shader/ui_vs.glsl"
            }
        }
        pub mod fs {
            vulkano_shaders::shader! {
                ty: "fragment",
                path: "src/shader/ui_fs.glsl"
            }
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    LoadErr(LoadingError),
    ValidationErr(Box<ValidationError>),
    VulkanErr(VulkanError),
    HandleErr(HandleError),
    AllocImgErr(AllocateImageError),
    AllocBufErr(AllocateBufferError),
    CmdBufExecErr(CommandBufferExecError),
    NoDevice,
    NoSrfFormats,
    NoGraphicsQueue,
    NoTransferQueue,
    BadShaderEntry,
    IoErr(std::io::Error),
    YmlErr(yml::Error),
    ObjErr(tobj::LoadError),
}
impl Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadErr(err) => writeln!(f, "Failed to load vulkan lib: {err}"),
            Self::ValidationErr(err) => writeln!(f, "Validation failed: {err}"),
            Self::VulkanErr(err) => writeln!(f, "Vulkan error: {err}"),
            Self::HandleErr(err) => writeln!(f, "Winit handle error: {err}"),
            Self::AllocImgErr(err) => writeln!(f, "Image allocation error: {err}"),
            Self::AllocBufErr(err) => writeln!(f, "Buffer allocation error: {err}"),
            Self::CmdBufExecErr(err) => writeln!(f, "Error executing command buffer: {err}"),
            Self::NoDevice => writeln!(f, "No device that supports vulkan"),
            Self::NoSrfFormats => writeln!(f, "Device has no supported surface formats"),
            Self::NoGraphicsQueue => writeln!(f, "No valid graphics queue"),
            Self::NoTransferQueue => writeln!(f, "No valid transfer queue"),
            Self::BadShaderEntry => writeln!(f, "Shader has bad entry point name"),
            Self::IoErr(err) => writeln!(f, "IO Error: {err}"),
            Self::YmlErr(err) => writeln!(f, "YAML Error: {err}"),
            Self::ObjErr(err) => writeln!(f, "Error loading OBJ: {err}"),
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
impl From<Validated<AllocateImageError>> for RenderError {
    fn from(value: Validated<AllocateImageError>) -> Self {
        match value {
            Validated::Error(err) => Self::AllocImgErr(err),
            Validated::ValidationError(err) => Self::ValidationErr(err),
        }
    }
}
impl From<Validated<AllocateBufferError>> for RenderError {
    fn from(value: Validated<AllocateBufferError>) -> Self {
        match value {
            Validated::Error(err) => Self::AllocBufErr(err),
            Validated::ValidationError(err) => Self::ValidationErr(err),
        }
    }
}
impl From<CommandBufferExecError> for RenderError {
    fn from(value: CommandBufferExecError) -> Self {
        Self::CmdBufExecErr(value)
    }
}
impl From<std::io::Error> for RenderError {
    fn from(value: std::io::Error) -> Self {
        Self::IoErr(value)
    }
}
impl From<yml::Error> for RenderError {
    fn from(value: yml::Error) -> Self {
        Self::YmlErr(value)
    }
}
impl From<tobj::LoadError> for RenderError {
    fn from(value: tobj::LoadError) -> Self {
        Self::ObjErr(value)
    }
}
impl From<HandleError> for RenderError {
    fn from(value: HandleError) -> Self {
        Self::HandleErr(value)
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
impl VertexData {
    fn new(pos: Vec3, norm: Vec3) -> Self {
        Self {
            pos,
            norm,
            data: [0; 4],
        }
    }
}

#[derive(Debug, Default, Clone, Copy, BufferContents, Vertex)]
#[repr(C)]
struct UIVertData {
    #[format(R32G32_SFLOAT)]
    pub pos: [f32; 2],
    #[format(R16G16_UNORM)]
    pub uv: [u16; 2],
    #[format(R8G8B8A8_UNORM)]
    pub color: [u8; 4],
}
impl From<&egui::epaint::Vertex> for UIVertData {
    fn from(value: &egui::epaint::Vertex) -> Self {
        Self {
            pos: [value.pos.x, value.pos.y],
            uv: [
                (value.uv.x * u16::MAX as f32) as u16,
                (value.uv.y * u16::MAX as f32) as u16,
            ],
            color: value.color.to_array(),
        }
    }
}

use shader::fore::vs::PushData;
use shader::{fore::fs::Material, ui};

#[derive(Debug)]
pub struct UpdateUI {
    pub textures_delta: egui::TexturesDelta,
    pub primitives: Vec<egui::ClippedPrimitive>,
}

#[derive(Debug)]
pub enum RenderMessage {
    Stop,
    UpdateUI(UpdateUI),
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
    pub max_framerate: Option<NonZeroU16>,
}

fn new_basic_image(
    alloc: Arc<StandardMemoryAllocator>,
    format: Format,
    extent: [u32; 3],
    tiling: ImageTiling,
    usage: ImageUsage,
) -> Result<Arc<ImageView>, RenderError> {
    let image = Image::new(
        alloc,
        ImageCreateInfo {
            format,
            extent,
            tiling,
            usage,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )?;

    Ok(ImageView::new_default(image.clone())?)
}

#[derive(Debug)]
struct MeshData {
    vert_buffer: Subbuffer<[VertexData]>,
    index_buffer: Subbuffer<[u16]>,
    entities: Vec<Entity>,
}

#[derive(Debug)]
struct MatData {
    //buffer: Subbuffer<Material>,
    desc_set: Arc<DescriptorSet>,
}

#[derive(Debug)]
struct UIBufferData {
    verts: Subbuffer<[UIVertData]>,
    idxs: Subbuffer<[u32]>,
    clip: egui::Rect,
}

#[derive(Debug)]
struct UIMatData {
    image: Arc<Image>,
    desc_set: Arc<DescriptorSet>,
    buffers: Vec<UIBufferData>,
}

/*
struct SkipDebug<T> {
    inner: T,
}
impl<T> SkipDebug<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}
impl<T> Debug for SkipDebug<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Skipped debug info for field of type: {}",
            std::any::type_name::<T>()
        )
    }
}
impl<T> Deref for SkipDebug<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<T> DerefMut for SkipDebug<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
*/

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct FramePush {
    proj: [[f32; 4]; 4],
    cam: pga::Motor,
}

#[derive(Debug)]
pub struct Render {
    receiver: Receiver<RenderMessage>,

    inverse_aspect_ratio: f32,

    min_frame_period: Option<Duration>,

    //instance: Arc<Instance>,
    device: Arc<Device>,
    graphics_queue: Arc<Queue>,
    transfer_queue: Arc<Queue>,

    swapchain: Arc<Swapchain>,
    //swap_imgs: Vec<Arc<Image>>,
    allocator: Arc<StandardMemoryAllocator>,

    framebuffers: Vec<Arc<Framebuffer>>,

    //render_pass: Arc<RenderPass>,
    desc_set_alloc: Arc<StandardDescriptorSetAllocator>,

    mat_desc_set_layout: Arc<DescriptorSetLayout>,

    defer_desc_set: Arc<DescriptorSet>,

    fore_pipeline: Arc<GraphicsPipeline>,
    defer_pipeline: Arc<GraphicsPipeline>,

    cmd_buffer_alloc: Arc<StandardCommandBufferAllocator>,

    components: Arc<Components>,

    mesh_name_to_id: AHashMap<String, NonZeroU64>,
    next_mesh_id: NonZeroU64,
    meshes: AHashMap<NonZeroU64, MeshData>,
    mat_name_to_id: AHashMap<String, NonZeroU64>,
    next_mat_id: NonZeroU64,
    mat_buffer: Subbuffer<[Material]>,
    materials: AHashMap<NonZeroU64, MatData>,

    ui_allocator: Arc<GenericMemoryAllocator<BumpAllocator>>,
    ui_tex_desc_set_layout: Arc<DescriptorSetLayout>,
    ui_static_desc_set: Arc<DescriptorSet>,
    ui_pipeline: Arc<GraphicsPipeline>,
    ui_mats: AHashMap<u64, UIMatData>,
}
impl System for Render {
    type Init = RenderInit;
    type InitErr = RenderError;
    type Err = RenderError;
    type Msg = RenderMessage;
    fn new(
        comps: &Arc<Components>,
        _: &Arc<RwLock<Input>>,
        RenderInit {
            window,
            res_x,
            res_y,
            max_framerate,
        }: RenderInit,
        receiver: Receiver<RenderMessage>,
    ) -> Result<Self, RenderError> {
        let library = VulkanLibrary::new()?;

        let required_extensions = Surface::required_extensions(&*window)?;

        let instance = Instance::new(library, {
            let mut info = InstanceCreateInfo::application_from_cargo_toml();
            info.engine_name = info.application_name.clone();
            info.engine_version = info.application_version.clone();
            info.enabled_extensions = required_extensions;
            info
        })?;

        let surface =
            Surface::from_window(instance.clone(), window.clone()).map_err(|e| match e {
                FromWindowError::RetrieveHandle(h) => RenderError::from(h),
                FromWindowError::CreateSurface(c) => RenderError::from(c),
            })?;

        let phys_device = instance
            .enumerate_physical_devices()?
            .find(|pd| pd.properties().device_type == PhysicalDeviceType::DiscreteGpu)
            .ok_or(RenderError::NoDevice)?;

        let &(srf_fmt, _srf_color_space) = phys_device
            .surface_formats(&surface, SurfaceInfo::default())?
            .first()
            .ok_or(RenderError::NoSrfFormats)?;

        log::info!("Surface format is: {:#?}", srf_fmt);

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

        let mut queue_create_infos = Vec::with_capacity(2);
        queue_create_infos.push(QueueCreateInfo {
            queue_family_index: graphics_queue_fam_id,
            ..Default::default()
        });
        if transfer_queue_fam_id != graphics_queue_fam_id {
            queue_create_infos.push(QueueCreateInfo {
                queue_family_index: transfer_queue_fam_id,
                ..Default::default()
            });
        }

        let (device, mut queue_iter) = Device::new(
            phys_device.clone(),
            DeviceCreateInfo {
                queue_create_infos: queue_create_infos,
                enabled_extensions: DeviceExtensions {
                    khr_swapchain: true,
                    ..Default::default()
                },
                //physical_devices: smallvec![],
                ..Default::default()
            },
        )?;
        let graphics_queue = queue_iter.next().unwrap();
        let transfer_queue = queue_iter.next().unwrap_or(graphics_queue.clone());

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

        let mat_desc_set_layout = DescriptorSetLayout::new(
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

        let desc_set_alloc = Arc::new(StandardDescriptorSetAllocator::new(
            device.clone(),
            StandardDescriptorSetAllocatorCreateInfo::default(),
        ));

        let fore_pipeline_layout = PipelineLayout::new(
            device.clone(),
            PipelineLayoutCreateInfo {
                set_layouts: vec![mat_desc_set_layout.clone()],
                push_constant_ranges: vec![PushConstantRange {
                    stages: ShaderStages::VERTEX,
                    offset: 0,
                    size: size_of::<PushData>() as u32,
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
                        format: Format::R16G16_UNORM,
                        load_op: AttachmentLoadOp::Clear,
                        final_layout: ImageLayout::ShaderReadOnlyOptimal,
                        ..Default::default()
                    },
                    // depth
                    AttachmentDescription {
                        format: Format::D32_SFLOAT,
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
                            layout: ImageLayout::DepthStencilAttachmentOptimal,
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
                                layout: ImageLayout::ShaderReadOnlyOptimal,
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
                    //UI
                    SubpassDescription {
                        color_attachments: vec![Some(AttachmentReference {
                            attachment: 4,
                            layout: ImageLayout::ColorAttachmentOptimal,
                            ..Default::default()
                        })],
                        ..Default::default()
                    },
                ],
                dependencies: vec![
                    SubpassDependency {
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
                    },
                    SubpassDependency {
                        src_subpass: Some(1),
                        dst_subpass: Some(2),
                        src_stages: PipelineStages::COLOR_ATTACHMENT_OUTPUT,
                        dst_stages: PipelineStages::COLOR_ATTACHMENT_OUTPUT,
                        src_access: AccessFlags::COLOR_ATTACHMENT_WRITE,
                        dst_access: AccessFlags::COLOR_ATTACHMENT_READ,
                        dependency_flags: DependencyFlags::BY_REGION,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        )?;

        let alloc = Arc::new(GenericMemoryAllocator::new_default(device.clone()));

        let opaque_color_img = new_basic_image(
            alloc.clone(),
            Format::R8G8B8A8_UNORM,
            [res_x as u32, res_y as u32, 1],
            ImageTiling::Optimal,
            ImageUsage::COLOR_ATTACHMENT | ImageUsage::INPUT_ATTACHMENT,
        )?;
        let opaque_pos_img = new_basic_image(
            alloc.clone(),
            Format::R32G32_SFLOAT,
            [res_x as u32, res_y as u32, 1],
            ImageTiling::Optimal,
            ImageUsage::COLOR_ATTACHMENT | ImageUsage::INPUT_ATTACHMENT,
        )?;
        let opaque_norm_img = new_basic_image(
            alloc.clone(),
            Format::R16G16_UNORM,
            [res_x as u32, res_y as u32, 1],
            ImageTiling::Optimal,
            ImageUsage::COLOR_ATTACHMENT | ImageUsage::INPUT_ATTACHMENT,
        )?;
        let depth_img = new_basic_image(
            alloc.clone(),
            Format::D32_SFLOAT,
            [res_x as u32, res_y as u32, 1],
            ImageTiling::Optimal,
            ImageUsage::DEPTH_STENCIL_ATTACHMENT | ImageUsage::INPUT_ATTACHMENT,
        )?;

        let framebuffers = swap_imgs
            .into_iter()
            .map(|img| {
                Framebuffer::new(
                    render_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![
                            opaque_color_img.clone(),
                            opaque_pos_img.clone(),
                            opaque_norm_img.clone(),
                            depth_img.clone(),
                            ImageView::new_default(img)?,
                        ],
                        extent: [res_x as u32, res_y as u32],
                        ..Default::default()
                    },
                )
            })
            .collect::<Result<Vec<Arc<Framebuffer>>, Validated<VulkanError>>>()?;

        let fore_pipeline = GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(fore_vs_entry.clone()),
                    PipelineShaderStageCreateInfo::new(fore_fs_entry),
                ],
                vertex_input_state: Some([VertexData::per_vertex()].definition(&fore_vs_entry)?),
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
                multisample_state: Some(MultisampleState::default()),
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

        let input_attach_desc_set_layout = DescriptorSetLayout::new(
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

        let defer_desc_set = DescriptorSet::new(
            desc_set_alloc.clone(),
            input_attach_desc_set_layout.clone(),
            [
                WriteDescriptorSet::image_view(0, opaque_color_img.clone()),
                WriteDescriptorSet::image_view(1, opaque_norm_img.clone()),
                WriteDescriptorSet::image_view(2, opaque_pos_img.clone()),
                WriteDescriptorSet::image_view(3, depth_img.clone()),
            ],
            [],
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
                set_layouts: vec![input_attach_desc_set_layout],
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
                multisample_state: Some(MultisampleState::default()),
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

        let cmd_buffer_alloc = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            StandardCommandBufferAllocatorCreateInfo {
                primary_buffer_count: 2,
                ..Default::default()
            },
        ));

        let sharing = if graphics_queue_fam_id == transfer_queue_fam_id {
            Sharing::Exclusive
        } else {
            Sharing::Concurrent(smallvec![graphics_queue_fam_id, transfer_queue_fam_id])
        };

        let mat_buffer = Buffer::new_slice(
            alloc.clone(),
            BufferCreateInfo {
                sharing,
                usage: BufferUsage::UNIFORM_BUFFER | BufferUsage::TRANSFER_DST,
                ..Default::default()
            },
            AllocationCreateInfo::default(),
            1024,
        )?;

        let ui_allocator = Arc::new(GenericMemoryAllocator::<BumpAllocator>::new(
            device.clone(),
            GenericMemoryAllocatorCreateInfo {
                block_sizes: &vec![2 << 20; phys_device.memory_properties().memory_types.len()],
                ..Default::default()
            },
        ));
        let ui_static_desc_set_layout = DescriptorSetLayout::new(
            device.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: [(
                    0,
                    DescriptorSetLayoutBinding {
                        stages: ShaderStages::VERTEX,
                        ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::UniformBuffer)
                    },
                )]
                .into(),
                ..Default::default()
            },
        )?;
        let ui_tex_desc_set_layout = DescriptorSetLayout::new(
            device.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: [(
                    0,
                    DescriptorSetLayoutBinding {
                        stages: ShaderStages::FRAGMENT,
                        ..DescriptorSetLayoutBinding::descriptor_type(
                            DescriptorType::CombinedImageSampler,
                        )
                    },
                )]
                .into(),
                ..Default::default()
            },
        )?;

        let staging_buf = Buffer::from_data(
            alloc.clone(),
            BufferCreateInfo {
                usage: BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            [res_x as f32, res_y as f32],
        )?;
        let ui_res_buf = Buffer::new_sized::<[f32; 2]>(
            alloc.clone(),
            BufferCreateInfo {
                usage: BufferUsage::TRANSFER_DST | BufferUsage::UNIFORM_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo::default(),
        )?;
        let mut cb_builder = AutoCommandBufferBuilder::primary(
            cmd_buffer_alloc.clone(),
            transfer_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;
        cb_builder.copy_buffer(CopyBufferInfo::buffers(staging_buf, ui_res_buf.clone()))?;

        let fence = cb_builder
            .build()?
            .execute(transfer_queue.clone())?
            .then_signal_fence_and_flush()?;

        let ui_static_desc_set = DescriptorSet::new(
            desc_set_alloc.clone(),
            ui_static_desc_set_layout.clone(),
            [WriteDescriptorSet::buffer(0, ui_res_buf)],
            [],
        )?;

        let ui_pipeline_layout = PipelineLayout::new(
            device.clone(),
            PipelineLayoutCreateInfo {
                set_layouts: vec![ui_static_desc_set_layout, ui_tex_desc_set_layout.clone()],
                ..Default::default()
            },
        )?;

        let ui_vs_entry = shader::ui::vs::load(device.clone())?
            .single_entry_point()
            .ok_or(RenderError::BadShaderEntry)?;
        let ui_fs_entry = shader::ui::fs::load(device.clone())?
            .single_entry_point()
            .ok_or(RenderError::BadShaderEntry)?;

        let ui_pipeline = GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(ui_vs_entry.clone()),
                    PipelineShaderStageCreateInfo::new(ui_fs_entry),
                ],
                vertex_input_state: Some([UIVertData::per_vertex()].definition(&ui_vs_entry)?),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState {
                    viewports: smallvec![Viewport {
                        extent: [res_x as f32, res_y as f32],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                rasterization_state: Some(RasterizationState {
                    cull_mode: CullMode::None,
                    ..Default::default()
                }),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    1,
                    ColorBlendAttachmentState {
                        blend: Some(AttachmentBlend {
                            dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                            dst_alpha_blend_factor: BlendFactor::OneMinusSrcAlpha,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )),
                dynamic_state: [DynamicState::Scissor].into_iter().collect(),
                subpass: Some(PipelineSubpassType::BeginRenderPass(
                    Subpass::from(render_pass.clone(), 2).unwrap(),
                )),
                ..GraphicsPipelineCreateInfo::layout(ui_pipeline_layout)
            },
        )?;

        fence.wait(None)?;

        Ok(Self {
            receiver,
            inverse_aspect_ratio: res_y as f32 / res_x as f32,
            min_frame_period: max_framerate.map(|fr| Duration::from_secs_f32((fr.get() as f32).recip())),
            device,
            graphics_queue,
            transfer_queue,
            swapchain,
            allocator: alloc,
            framebuffers,
            mat_desc_set_layout,
            desc_set_alloc,
            defer_desc_set,
            fore_pipeline,
            defer_pipeline,
            cmd_buffer_alloc,
            components: comps.clone(),
            mesh_name_to_id: AHashMap::default(),
            next_mesh_id: NonZeroU64::new(1).unwrap(),
            meshes: AHashMap::new(),
            mat_name_to_id: AHashMap::default(),
            next_mat_id: NonZeroU64::new(1).unwrap(),
            mat_buffer,
            materials: AHashMap::default(),

            ui_allocator,
            ui_tex_desc_set_layout,
            ui_static_desc_set,
            ui_pipeline,
            ui_mats: AHashMap::default(),
        })
    }
    fn run(&mut self) -> Result<(), RenderError> {
        let mut prev_fence: Option<
            FenceSignalFuture<PresentFuture<CommandBufferExecFuture<Box<dyn GpuFuture>>>>,
        > = None;

        let mut ui_updates = SmallVec::<[UpdateUI;1]>::new();
        let mut next_time = Instant::now();
        log::debug!("{:#?}", self.min_frame_period);
        loop {
            if let Some(frame_period) = self.min_frame_period {
                let now = Instant::now();
                if let Some(to_sleep) = next_time.checked_duration_since(now) {
                    std::thread::sleep(to_sleep);
                }
                next_time = now + frame_period;
            }
            log::trace!("frame start");
            ui_updates.clear();
            for msg in self.receiver.try_iter() {
                match msg {
                    RenderMessage::Stop => return Ok(()),
                    RenderMessage::UpdateUI(update_ui) => ui_updates.push(update_ui),
                }
            }

            for (e, instance) in self
                .components
                .static_mesh_instances
                .write()
                .unwrap()
                .iter_mut()
            {
                if instance.mesh_id.is_none() {
                    let id = match self.mesh_name_to_id.entry(instance.mesh_name.clone()) {
                        Entry::Occupied(occupied) => occupied.get().clone(),
                        Entry::Vacant(vacant) => {
                            let mesh_id = self.next_mesh_id;
                            self.next_mesh_id = self.next_mesh_id.checked_add(1).unwrap();
                            vacant.insert(mesh_id);

                            let (models, _) = tobj::load_obj(
                                format!("assets/meshes/{}.obj", instance.mesh_name),
                                &tobj::GPU_LOAD_OPTIONS,
                            )?;
                            assert!(
                                models.len() == 1,
                                "Odd number of models in obj file: {}",
                                models.len()
                            );

                            if models[0].name != instance.mesh_name {
                                log::warn!(
                                    "OBJ Model name \"{}\" differs from mesh name \"{}\"",
                                    &models[0].name,
                                    &instance.mesh_name
                                );
                            }

                            let mesh = &models[0].mesh;

                            let vert_src = Buffer::from_iter(
                                self.allocator.clone(),
                                BufferCreateInfo {
                                    usage: BufferUsage::TRANSFER_SRC,
                                    ..Default::default()
                                },
                                AllocationCreateInfo {
                                    memory_type_filter: MemoryTypeFilter::PREFER_HOST
                                        | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                    ..Default::default()
                                },
                                mesh.positions.chunks(3).zip(mesh.normals.chunks(3)).map(
                                    |(pos, norm)| {
                                        VertexData::new(
                                            Vec3::new(pos[0], pos[1], pos[2]),
                                            Vec3::new(norm[0], norm[1], norm[2]),
                                        )
                                    },
                                ),
                            )?;
                            let vert_dst: Subbuffer<[VertexData]> = Buffer::new_slice(
                                self.allocator.clone(),
                                BufferCreateInfo {
                                    usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
                                    ..Default::default()
                                },
                                AllocationCreateInfo::default(),
                                mesh.positions.len() as u64,
                            )?;

                            let index_src = Buffer::from_iter(
                                self.allocator.clone(),
                                BufferCreateInfo {
                                    usage: BufferUsage::TRANSFER_SRC,
                                    ..Default::default()
                                },
                                AllocationCreateInfo {
                                    memory_type_filter: MemoryTypeFilter::PREFER_HOST
                                        | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                    ..Default::default()
                                },
                                mesh.indices.iter().map(|&i| i as u16),
                            )?;
                            let index_dst: Subbuffer<[u16]> = Buffer::new_slice(
                                self.allocator.clone(),
                                BufferCreateInfo {
                                    usage: BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST,
                                    ..Default::default()
                                },
                                AllocationCreateInfo::default(),
                                mesh.indices.len() as u64,
                            )?;

                            let mut cb_builder = AutoCommandBufferBuilder::primary(
                                self.cmd_buffer_alloc.clone(),
                                self.transfer_queue.queue_family_index(),
                                CommandBufferUsage::OneTimeSubmit,
                            )?;
                            cb_builder
                                .copy_buffer(CopyBufferInfoTyped::buffers(
                                    vert_src,
                                    vert_dst.clone(),
                                ))?
                                .copy_buffer(CopyBufferInfoTyped::buffers(
                                    index_src,
                                    index_dst.clone(),
                                ))?;

                            let fence = cb_builder
                                .build()?
                                .execute(self.transfer_queue.clone())?
                                .then_signal_fence_and_flush()?;

                            self.meshes.insert(
                                mesh_id,
                                MeshData {
                                    vert_buffer: vert_dst,
                                    index_buffer: index_dst,
                                    entities: vec![e],
                                },
                            );

                            fence.wait(None)?;

                            mesh_id
                        }
                    };
                    self.meshes.get_mut(&id).unwrap().entities.push(e);
                    instance.mesh_id = Some(id);
                }
                if instance.material_id.is_none() {
                    let id = match self.mat_name_to_id.entry(instance.material_name.clone()) {
                        Entry::Occupied(occupied) => occupied.get().clone(),
                        Entry::Vacant(vacant) => {
                            let mat_id = self.next_mat_id;
                            self.next_mat_id = self.next_mat_id.checked_add(1).unwrap();
                            vacant.insert(mat_id);

                            let material: Material = yml::from_reader(File::open(format!(
                                "assets/materials/{}.yaml",
                                &instance.material_name
                            ))?)?;

                            let src_buf = Buffer::from_data(
                                self.allocator.clone(),
                                BufferCreateInfo {
                                    usage: BufferUsage::TRANSFER_SRC,
                                    ..Default::default()
                                },
                                AllocationCreateInfo {
                                    memory_type_filter: MemoryTypeFilter::PREFER_HOST
                                        | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                    ..Default::default()
                                },
                                material,
                            )?;
                            let dst_buf = self.mat_buffer.clone().index(mat_id.get());

                            let mut cb_builder = AutoCommandBufferBuilder::primary(
                                self.cmd_buffer_alloc.clone(),
                                self.transfer_queue.queue_family_index(),
                                CommandBufferUsage::OneTimeSubmit,
                            )?;
                            cb_builder
                                .copy_buffer(CopyBufferInfo::buffers(src_buf, dst_buf.clone()))?;

                            let fence = cb_builder
                                .build()?
                                .execute(self.transfer_queue.clone())?
                                .then_signal_fence_and_flush()?;

                            let desc_set = DescriptorSet::new(
                                self.desc_set_alloc.clone(),
                                self.mat_desc_set_layout.clone(),
                                [WriteDescriptorSet::buffer(0, dst_buf.clone())],
                                [],
                            )?;

                            self.materials.insert(
                                mat_id,
                                MatData {
                                    //buffer: dst_buf,
                                    desc_set,
                                },
                            );

                            fence.wait(None)?;

                            mat_id
                        }
                    };
                    instance.material_id = Some(id);
                }
            }

            if let Some(fence) = prev_fence.take() {
                fence.wait(None)?;
            }

            let ui_fence = if !ui_updates.is_empty() {
                let mut cb_builder = AutoCommandBufferBuilder::primary(
                    self.cmd_buffer_alloc.clone(),
                    self.transfer_queue.queue_family_index(),
                    CommandBufferUsage::OneTimeSubmit,
                )?;

                for ui_update in &ui_updates {
                    if !ui_update.textures_delta.is_empty() {
                        for (tex_id, img_delta) in &ui_update.textures_delta.set {
                            let tex_id = match tex_id {
                                TextureId::Managed(v) => *v,
                                TextureId::User(_) => {
                                    unimplemented!("User texture ids not supported")
                                }
                            };
                            let entry = match self.ui_mats.entry(tex_id) {
                                Entry::Vacant(v) => {
                                    let format = Format::R8G8B8A8_SRGB;
                                    let image = Image::new(
                                        self.allocator.clone(),
                                        ImageCreateInfo {
                                            image_type: ImageType::Dim2d,
                                            format,
                                            view_formats: vec![format],
                                            extent: [
                                                img_delta.image.width() as u32,
                                                img_delta.image.height() as u32,
                                                1,
                                            ],
                                            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
                                            ..Default::default()
                                        },
                                        AllocationCreateInfo::default(),
                                    )?;
                                    let view = ImageView::new_default(image.clone())?;
                                    let sampler = Sampler::new(
                                        self.device.clone(),
                                        SamplerCreateInfo {
                                            mag_filter: match img_delta.options.magnification {
                                                egui::TextureFilter::Nearest => Filter::Nearest,
                                                egui::TextureFilter::Linear => Filter::Linear,
                                            },
                                            min_filter: match img_delta.options.minification {
                                                egui::TextureFilter::Nearest => Filter::Nearest,
                                                egui::TextureFilter::Linear => Filter::Linear,
                                            },
                                            mipmap_mode: match img_delta
                                                .options
                                                .mipmap_mode
                                                .unwrap_or(egui::TextureFilter::Nearest)
                                            {
                                                egui::TextureFilter::Nearest => {
                                                    SamplerMipmapMode::Nearest
                                                }
                                                egui::TextureFilter::Linear => {
                                                    SamplerMipmapMode::Linear
                                                }
                                            },
                                            address_mode: [match img_delta.options.wrap_mode {
                                                egui::TextureWrapMode::ClampToEdge => {
                                                    SamplerAddressMode::ClampToEdge
                                                }
                                                egui::TextureWrapMode::Repeat => {
                                                    SamplerAddressMode::Repeat
                                                }
                                                egui::TextureWrapMode::MirroredRepeat => {
                                                    SamplerAddressMode::MirroredRepeat
                                                }
                                            };
                                                3],
                                            ..Default::default()
                                        },
                                    )?;

                                    let desc_set = DescriptorSet::new(
                                        self.desc_set_alloc.clone(),
                                        self.ui_tex_desc_set_layout.clone(),
                                        [WriteDescriptorSet::image_view_sampler(0, view, sampler)],
                                        [],
                                    )?;
                                    v.insert_entry(UIMatData {
                                        image,
                                        desc_set,
                                        buffers: Vec::new(),
                                    })
                                }
                                Entry::Occupied(o) => o,
                            };

                            let staging_buf = match &img_delta.image {
                                egui::ImageData::Color(img) => Buffer::from_iter(
                                    self.allocator.clone(),
                                    BufferCreateInfo {
                                        usage: BufferUsage::TRANSFER_SRC,
                                        ..Default::default()
                                    },
                                    AllocationCreateInfo {
                                        memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                        ..Default::default()
                                    },
                                    img.pixels.iter().copied(),
                                )?,
                                egui::ImageData::Font(img) => Buffer::from_iter(
                                    self.allocator.clone(),
                                    BufferCreateInfo {
                                        usage: BufferUsage::TRANSFER_SRC,
                                        ..Default::default()
                                    },
                                    AllocationCreateInfo {
                                        memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                        ..Default::default()
                                    },
                                    img.srgba_pixels(None),
                                )?,
                            };

                            let mut copy_info = CopyBufferToImageInfo::buffer_image(
                                staging_buf,
                                entry.get().image.clone(),
                            );
                            if let Some(region) = img_delta.pos {
                                copy_info.regions[0] = BufferImageCopy {
                                    buffer_row_length: img_delta.image.width() as u32,
                                    buffer_image_height: img_delta.image.height() as u32,
                                    image_subresource: ImageSubresourceLayers::from_parameters(
                                        entry.get().image.format(),
                                        1,
                                    ),
                                    image_offset: [region[0] as u32, region[1] as u32, 0],
                                    image_extent: [
                                        img_delta.image.width() as u32,
                                        img_delta.image.height() as u32,
                                        1,
                                    ],
                                    ..Default::default()
                                };
                            }
                            cb_builder.copy_buffer_to_image(copy_info)?;
                        }
                    }

                    for ui_mat in self.ui_mats.values_mut() {
                        ui_mat.buffers.clear();
                    }

                    for primitive in &ui_update.primitives {
                        match &primitive.primitive {
                            egui::epaint::Primitive::Mesh(mesh) => {
                                let ui_mat = self.ui_mats.get_mut(match &mesh.texture_id {
                                    egui::epaint::TextureId::Managed(v) => v,
                                    egui::epaint::TextureId::User(_) => unimplemented!(),
                                });
                                if ui_mat.is_none() {
                                    log::warn!(
                                        "UI Material id \"{:#?}\" not found, skipping mesh",
                                        mesh.texture_id
                                    );
                                    continue;
                                }
                                let ui_mat = ui_mat.unwrap();

                                let vert_staging = Buffer::from_iter(
                                    self.ui_allocator.clone(),
                                    BufferCreateInfo {
                                        usage: BufferUsage::TRANSFER_SRC,
                                        ..Default::default()
                                    },
                                    AllocationCreateInfo {
                                        memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                        ..Default::default()
                                    },
                                    mesh.vertices.iter().map(|v| UIVertData::from(v)),
                                )?;

                                let verts = Buffer::new_slice::<UIVertData>(
                                    self.ui_allocator.clone(),
                                    BufferCreateInfo {
                                        usage: BufferUsage::VERTEX_BUFFER
                                            | BufferUsage::TRANSFER_DST,
                                        ..Default::default()
                                    },
                                    AllocationCreateInfo::default(),
                                    vert_staging.len(),
                                )?;

                                cb_builder.copy_buffer(CopyBufferInfoTyped::buffers(
                                    vert_staging,
                                    verts.clone(),
                                ))?;

                                let idx_staging = Buffer::from_iter(
                                    self.ui_allocator.clone(),
                                    BufferCreateInfo {
                                        usage: BufferUsage::TRANSFER_SRC,
                                        ..Default::default()
                                    },
                                    AllocationCreateInfo {
                                        memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                                        ..Default::default()
                                    },
                                    mesh.indices.iter().copied(),
                                )?;

                                let idxs = Buffer::new_slice::<u32>(
                                    self.ui_allocator.clone(),
                                    BufferCreateInfo {
                                        usage: BufferUsage::INDEX_BUFFER
                                            | BufferUsage::TRANSFER_DST,
                                        ..Default::default()
                                    },
                                    AllocationCreateInfo::default(),
                                    idx_staging.len(),
                                )?;

                                cb_builder.copy_buffer(CopyBufferInfoTyped::buffers(
                                    idx_staging,
                                    idxs.clone(),
                                ))?;

                                ui_mat.buffers.push(UIBufferData {
                                    verts,
                                    idxs,
                                    clip: primitive.clip_rect,
                                });
                            }
                            egui::epaint::Primitive::Callback(_cb) => unimplemented!(),
                        }
                    }
                }

                Some(
                    cb_builder
                        .build()?
                        .execute(self.transfer_queue.clone())?
                        .then_signal_fence_and_flush()?,
                )
            } else {
                None
            };

            let (img_index, suboptimal, acquire_future) =
                match acquire_next_image(self.swapchain.clone(), None) {
                    Ok(res) => res,
                    Err(v_err) => match v_err {
                        Validated::ValidationError(e) => return Err(e.into()),
                        Validated::Error(e) => match e {
                            VulkanError::OutOfDate => {
                                todo!();
                            }
                            other_err => return Err(other_err.into()),
                        },
                    },
                };

            if suboptimal {
                log::warn!("Suboptimal swapchain");
            }

            let mut builder = AutoCommandBufferBuilder::primary(
                self.cmd_buffer_alloc.clone(),
                self.graphics_queue.queue_family_index(),
                CommandBufferUsage::OneTimeSubmit,
            )?;

            builder.begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![
                        Some(ClearValue::Float([0.0, 0.0, 1.0, 1.0])),
                        Some(ClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                        Some(ClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                        Some(ClearValue::Depth(0.0)),
                        Some(ClearValue::Float([1.0, 0.0, 0.0, 1.0])),
                    ],
                    ..RenderPassBeginInfo::framebuffer(
                        self.framebuffers[img_index as usize].clone(),
                    )
                },
                SubpassBeginInfo::default(),
            )?;

            let maybe_cam_data = self
                .components
                .cameras
                .read()
                .unwrap()
                .get_one()
                .map(|(e, c)| (e, *c));
            if let Some((e, cam)) = maybe_cam_data {
                let cam_transform = self
                    .components
                    .transforms
                    .read()
                    .unwrap()
                    .get(e)
                    .unwrap()
                    .clone();
                let inv_cam = cam_transform
                    .global_motor(&self.components.transforms.read().unwrap())
                    .inverse();
                let proj_factor = 1.0 / (cam.fov * 0.5).tan();

                builder
                    .bind_pipeline_graphics(self.fore_pipeline.clone())?
                    .push_constants(
                        self.fore_pipeline.layout().clone(),
                        0,
                        FramePush {
                            proj: [
                                [self.inverse_aspect_ratio * proj_factor, 0.0, 0.0, 0.0],
                                [0.0, -proj_factor, 0.0, 0.0],
                                [0.0, 0.0, 0.0, -1.0],
                                [0.0, 0.0, cam.near_plane, 0.0],
                            ],
                            cam: inv_cam,
                        },
                    )?;
                {
                    let transforms = self.components.transforms.read().unwrap();
                    for (_, mat_data) in &self.materials {
                        builder.bind_descriptor_sets(
                            PipelineBindPoint::Graphics,
                            self.fore_pipeline.layout().clone(),
                            0,
                            mat_data.desc_set.clone(),
                        )?;
                        for (_, mesh) in &self.meshes {
                            builder
                                .bind_vertex_buffers(0, mesh.vert_buffer.clone())?
                                .bind_index_buffer(mesh.index_buffer.clone())?;
                            for &e in &mesh.entities {
                                builder.push_constants(
                                    self.fore_pipeline.layout().clone(),
                                    offset_of!(PushData, obj) as u32,
                                    transforms
                                        .get(e)
                                        .expect(&format!(
                                            "Expected tranform of entity {e} in render"
                                        ))
                                        .global_motor(&transforms),
                                )?;
                                unsafe {
                                    builder.draw_indexed(
                                        mesh.index_buffer.len() as u32,
                                        1,
                                        0,
                                        0,
                                        0,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            builder
                .next_subpass(SubpassEndInfo::default(), SubpassBeginInfo::default())?
                .bind_pipeline_graphics(self.defer_pipeline.clone())?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    self.defer_pipeline.layout().clone(),
                    0,
                    self.defer_desc_set.clone(),
                )?;
            unsafe {
                builder.draw(3, 1, 0, 0)?;
            }

            builder
                .next_subpass(SubpassEndInfo::default(), SubpassBeginInfo::default())?
                .bind_pipeline_graphics(self.ui_pipeline.clone())?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    self.ui_pipeline.layout().clone(),
                    0,
                    self.ui_static_desc_set.clone(),
                )?;
            for ui_mat in self.ui_mats.values() {
                builder.bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    self.ui_pipeline.layout().clone(),
                    1,
                    ui_mat.desc_set.clone(),
                )?;
                for buffers in &ui_mat.buffers {
                    let scissor = Scissor {
                        offset: [buffers.clip.min.x as u32, buffers.clip.min.y as u32],
                        extent: [
                            (buffers.clip.max.x - buffers.clip.min.x) as u32,
                            (buffers.clip.max.y - buffers.clip.min.y) as u32,
                        ],
                    };
                    builder
                        .set_scissor(0, smallvec![scissor])?
                        .bind_index_buffer(buffers.idxs.clone())?
                        .bind_vertex_buffers(0, buffers.verts.clone())?;
                    unsafe {
                        builder.draw_indexed(buffers.idxs.len() as u32, 1, 0, 0, 0)?;
                    }
                }
            }

            builder.end_render_pass(SubpassEndInfo::default())?;

            let cb = builder.build()?;

            let mut f_tmp: Box<dyn GpuFuture> =
                Box::new(vulkano::sync::now(self.device.clone()).join(acquire_future));
            if let Some(ui_fut) = ui_fence {
                f_tmp = Box::new(f_tmp.join(ui_fut));
            }

            prev_fence = Some(
                f_tmp
                    .then_execute(self.graphics_queue.clone(), cb)?
                    .then_swapchain_present(
                        self.graphics_queue.clone(),
                        SwapchainPresentInfo::swapchain_image_index(
                            self.swapchain.clone(),
                            img_index,
                        ),
                    )
                    .then_signal_fence_and_flush()?,
            );

            for ui_update in &ui_updates {
                for free_id in &ui_update.textures_delta.free {
                    match free_id {
                        TextureId::Managed(id) => self.ui_mats.remove(id),
                        TextureId::User(_) => unimplemented!(),
                    };
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StaticMeshInstance {
    pub mesh_name: String,
    pub material_name: String,
    #[serde(skip)]
    mesh_id: Option<NonZeroU64>,
    #[serde(skip)]
    material_id: Option<NonZeroU64>,
}
impl StaticMeshInstance {
    pub fn new(mesh_name: String, material_name: String) -> Self {
        Self {
            mesh_name,
            material_name,
            mesh_id: None,
            material_id: None,
        }
    }
}
impl Component for StaticMeshInstance {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Camera {
    pub fov: f32,
    pub near_plane: f32,
}
impl Camera {
    pub fn new(fov: f32, near_plane: f32) -> Self {
        Self { fov, near_plane }
    }
}
impl Component for Camera {}
