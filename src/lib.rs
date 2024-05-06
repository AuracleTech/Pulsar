mod camera;
mod metrics;
mod model;
mod shaders;

use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    util::Align,
    vk, Device, Entry, Instance,
};
use glam::{Mat4, Vec4};
use log::debug;
use metrics::Metrics;
use model::{Mesh, RegisteredMesh, Vertex};
use rand::Rng;
use shaders::Shader;
use std::{
    borrow::Cow, default::Default, error::Error, ffi, mem, ops::Drop, os::raw::c_char,
    thread::JoinHandle,
};
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoop,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::WindowBuilder,
};

const APP_NAME: &str = "Nhope Engine";
const APP_VERSION: &str = "0.1.0";

#[derive(Clone, Debug, Copy)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub _pad: f32,
}

#[macro_export]
macro_rules! offset_of {
    ($self:path, $field:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let b: $self = mem::zeroed();
            std::ptr::addr_of!(b.$field) as isize - std::ptr::addr_of!(b) as isize
        }
    }};
}

/// Helper function for submitting command buffers. Immediately waits for the fence before the command buffer
/// is executed. That way we can delay the waiting for the fences by 1 frame which is good for performance.
/// Make sure to create the fence in a signaled state on the first use.
fn record_submit_commandbuffer<F: FnOnce(&Device, vk::CommandBuffer)>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    command_buffer_reuse_fence: vk::Fence,
    submit_queue: vk::Queue,
    wait_mask: &[vk::PipelineStageFlags],
    wait_semaphores: &[vk::Semaphore],
    signal_semaphores: &[vk::Semaphore],
    f: F,
) {
    unsafe {
        device
            .wait_for_fences(&[command_buffer_reuse_fence], true, u64::MAX)
            .expect("Wait for fence failed.");

        device
            .reset_fences(&[command_buffer_reuse_fence])
            .expect("Reset fences failed.");

        device
            .reset_command_buffer(
                command_buffer,
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
            .expect("Reset command buffer failed.");

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        device
            .begin_command_buffer(command_buffer, &command_buffer_begin_info)
            .expect("Begin commandbuffer");
        f(device, command_buffer);
        device
            .end_command_buffer(command_buffer)
            .expect("End commandbuffer");

        let command_buffers = vec![command_buffer];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(signal_semaphores);

        device
            .queue_submit(submit_queue, &[submit_info], command_buffer_reuse_fence)
            .expect("queue submit failed.");
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    debug!(
        "{message_severity:?}: {message_type:?} [{message_id_name} ({message_id_number})] : {message}",
    );

    vk::FALSE
}

fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

struct EngineSurface {
    surface_khr: vk::SurfaceKHR,
    format: vk::SurfaceFormatKHR,
    capabilities: vk::SurfaceCapabilitiesKHR,
    resolution: vk::Extent2D,
}

struct EngineSwapchain {
    swapchain_khr: vk::SwapchainKHR,
    desired_image_count: u32,
    present_mode: vk::PresentModeKHR,
    present_queue: vk::Queue,
}

struct SwapchainResources {
    pool: vk::CommandPool,
    draw_command_buffer: vk::CommandBuffer,
    setup_command_buffer: vk::CommandBuffer,

    depth_image: vk::Image,
    depth_image_view: vk::ImageView,
    depth_image_memory: vk::DeviceMemory,

    present_images: Vec<vk::Image>,
    present_image_views: Vec<vk::ImageView>,

    draw_commands_reuse_fence: vk::Fence,
    setup_commands_reuse_fence: vk::Fence,

    present_complete_semaphore: vk::Semaphore,
    rendering_complete_semaphore: vk::Semaphore,
}

pub struct Engine {
    rng: rand::rngs::ThreadRng,

    window: winit::window::Window,
    entry: Entry,

    instance: Instance,
    pdevices: Vec<vk::PhysicalDevice>,
    pdevice: vk::PhysicalDevice,
    surface_loader: surface::Instance,

    debug_utils_loader: debug_utils::Instance,
    debug_call_back: vk::DebugUtilsMessengerEXT,

    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,

    surface: EngineSurface,

    swapchain_loader: swapchain::Device,
    swapchain: EngineSwapchain,
    swapchain_resources: SwapchainResources,

    device: Device,

    renderpass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    graphic_pipeline: vk::Pipeline,
    viewports: [vk::Viewport; 1],
    scissors: [vk::Rect2D; 1],
    graphics_pipelines: Vec<vk::Pipeline>,
    pipeline_layout: vk::PipelineLayout,
    vertex_shader_module: vk::ShaderModule,
    fragment_shader_module: vk::ShaderModule,

    descriptor_sets: Vec<vk::DescriptorSet>,
    desc_set_layouts: [vk::DescriptorSetLayout; 1],
    image_buffer_memory: vk::DeviceMemory,
    image_buffer: vk::Buffer,
    texture_memory: vk::DeviceMemory,
    tex_image_view: vk::ImageView,
    texture_image: vk::Image,
    uniform_color_buffer: vk::Buffer,
    uniform_buffer_memory: vk::DeviceMemory,
    descriptor_pool: vk::DescriptorPool,
    texture_sampler: vk::Sampler,

    registered_meshes: Vec<RegisteredMesh>,

    minimized: bool,

    metrics: Metrics,

    uniform: Mat4,
}

impl Engine {
    pub fn new(
        window_width: u32,
        window_height: u32,
    ) -> Result<(Self, EventLoop<()>), Box<dyn Error>> {
        env_logger::init();

        // TODO seeder RNG only
        let mut rng = rand::thread_rng();

        #[cfg(debug_assertions)]
        Shader::on_start_compile_shaders();

        unsafe {
            let event_loop = EventLoop::new()?;
            let window = WindowBuilder::new()
                .with_title(APP_NAME)
                .with_inner_size(winit::dpi::LogicalSize::new(
                    f64::from(window_width),
                    f64::from(window_height),
                ))
                .build(&event_loop)
                .unwrap();
            let entry = Entry::linked();

            let instance = Engine::create_instance(&entry, &window)?;

            let surface_loader = surface::Instance::new(&entry, &instance);

            let (debug_utils_loader, debug_call_back) =
                Engine::create_debug_utils_messenger(&entry, &instance)?;

            let pdevices = instance
                .enumerate_physical_devices()
                .expect("Physical device error");

            let (surface, pdevice, queue_family_index) =
                Engine::create_surface(&entry, &instance, &window, &pdevices, &surface_loader)?;

            let device = Engine::create_device(&instance, pdevice, queue_family_index)?;

            let swapchain_loader = swapchain::Device::new(&instance, &device);

            let swapchain = Engine::create_swapchain(
                &device,
                &surface_loader,
                &surface,
                pdevice,
                queue_family_index,
                window.inner_size(),
                &swapchain_loader,
            )?;

            let (draw_commands_reuse_fence, setup_commands_reuse_fence) =
                Engine::create_fences(&device)?;

            let (
                present_images,
                present_image_views,
                depth_image_view,
                depth_image,
                depth_image_memory,
                device_memory_properties,
            ) = Engine::create_views_and_depth(
                &device,
                &instance,
                &swapchain,
                &surface,
                &pdevice,
                &swapchain_loader,
            )?;

            let (present_complete_semaphore, rendering_complete_semaphore) =
                Engine::create_semaphores(&device)?;

            let renderpass = Engine::create_renderpass(&surface, &device)?;

            // MARK: DESCRIPTOR SET
            let descriptor_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                },
            ];
            let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&descriptor_sizes)
                .max_sets(1);

            let descriptor_pool = device
                .create_descriptor_pool(&descriptor_pool_info, None)
                .unwrap();
            let desc_layout_bindings = [
                vk::DescriptorSetLayoutBinding {
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                    stage_flags: vk::ShaderStageFlags::VERTEX,
                    ..Default::default()
                },
                vk::DescriptorSetLayoutBinding {
                    binding: 1,
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                    ..Default::default()
                },
            ];
            let descriptor_info =
                vk::DescriptorSetLayoutCreateInfo::default().bindings(&desc_layout_bindings);

            let desc_set_layouts = [device
                .create_descriptor_set_layout(&descriptor_info, None)
                .unwrap()];

            let desc_alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&desc_set_layouts);
            let descriptor_sets = device.allocate_descriptor_sets(&desc_alloc_info).unwrap();
            // MARK: END DESCRIPTOR SET

            let (
                graphic_pipeline,
                viewports,
                scissors,
                graphics_pipelines,
                pipeline_layout,
                vertex_shader_module,
                fragment_shader_module,
            ) = Engine::create_pipeline(&device, &surface, renderpass, desc_set_layouts)?;

            let framebuffers = Engine::create_framebuffers(
                &device,
                &surface,
                &present_image_views,
                depth_image_view,
                renderpass,
            )?;

            let pool = Engine::create_command_pools(&device, queue_family_index)?;

            let (setup_command_buffer, draw_command_buffer) =
                Engine::create_command_buffers(&device, pool)?;

            record_submit_commandbuffer(
                &device,
                setup_command_buffer,
                setup_commands_reuse_fence,
                swapchain.present_queue,
                &[],
                &[],
                &[],
                |device, setup_command_buffer| {
                    let layout_transition_barriers = vk::ImageMemoryBarrier::default()
                        .image(depth_image)
                        .dst_access_mask(
                            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        )
                        .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                                .layer_count(1)
                                .level_count(1),
                        );

                    device.cmd_pipeline_barrier(
                        setup_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[layout_transition_barriers],
                    );
                },
            );

            // MARK: UNIFORM BUFFER
            let mut uniform = Mat4::IDENTITY;
            // TEMP: rotate UBO transfrom
            uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, std::f32::consts::PI / 4.0);

            let (uniform_color_buffer, uniform_color_buffer_memory) =
                Engine::create_uniform_buffer(&device, &device_memory_properties, uniform);

            // MARK: IMAGE
            let image = image::load_from_memory(include_bytes!("../assets/img/picture.png"))
                .unwrap()
                .to_rgba8();
            let (width, height) = image.dimensions();
            let image_extent = vk::Extent2D { width, height };
            let image_data = image.into_raw();
            let image_buffer_info = vk::BufferCreateInfo {
                size: (mem::size_of::<u8>() * image_data.len()) as u64,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                ..Default::default()
            };
            let image_buffer = device.create_buffer(&image_buffer_info, None).unwrap();
            let image_buffer_memory_req = device.get_buffer_memory_requirements(image_buffer);
            let image_buffer_memory_index = find_memorytype_index(
                &image_buffer_memory_req,
                &device_memory_properties,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
            .expect("Unable to find suitable memorytype for the image buffer.");

            let image_buffer_allocate_info = vk::MemoryAllocateInfo {
                allocation_size: image_buffer_memory_req.size,
                memory_type_index: image_buffer_memory_index,
                ..Default::default()
            };
            let image_buffer_memory = device
                .allocate_memory(&image_buffer_allocate_info, None)
                .unwrap();
            let image_ptr = device
                .map_memory(
                    image_buffer_memory,
                    0,
                    image_buffer_memory_req.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();
            let mut image_slice = Align::new(
                image_ptr,
                mem::align_of::<u8>() as u64,
                image_buffer_memory_req.size,
            );
            image_slice.copy_from_slice(&image_data);
            device.unmap_memory(image_buffer_memory);
            device
                .bind_buffer_memory(image_buffer, image_buffer_memory, 0)
                .unwrap();

            // MARK: TEXTURE
            let texture_create_info = vk::ImageCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                format: vk::Format::R8G8B8A8_UNORM,
                extent: image_extent.into(),
                mip_levels: 1,
                array_layers: 1,
                samples: vk::SampleCountFlags::TYPE_1,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                ..Default::default()
            };
            let texture_image = device.create_image(&texture_create_info, None).unwrap();
            let texture_memory_req = device.get_image_memory_requirements(texture_image);
            let texture_memory_index = find_memorytype_index(
                &texture_memory_req,
                &device_memory_properties,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .expect("Unable to find suitable memory index for depth image.");

            let texture_allocate_info = vk::MemoryAllocateInfo {
                allocation_size: texture_memory_req.size,
                memory_type_index: texture_memory_index,
                ..Default::default()
            };
            let texture_memory = device
                .allocate_memory(&texture_allocate_info, None)
                .unwrap();
            device
                .bind_image_memory(texture_image, texture_memory, 0)
                .expect("Unable to bind depth image memory");

            // MARK: REC TEXTURE
            record_submit_commandbuffer(
                &device,
                setup_command_buffer,
                setup_commands_reuse_fence,
                swapchain.present_queue,
                &[],
                &[],
                &[],
                |device, texture_command_buffer| {
                    let texture_barrier = vk::ImageMemoryBarrier {
                        dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                        new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        image: texture_image,
                        subresource_range: vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            level_count: 1,
                            layer_count: 1,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    device.cmd_pipeline_barrier(
                        texture_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[texture_barrier],
                    );
                    let buffer_copy_regions = vk::BufferImageCopy::default()
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(image_extent.into());

                    device.cmd_copy_buffer_to_image(
                        texture_command_buffer,
                        image_buffer,
                        texture_image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[buffer_copy_regions],
                    );
                    let texture_barrier_end = vk::ImageMemoryBarrier {
                        src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                        dst_access_mask: vk::AccessFlags::SHADER_READ,
                        old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        image: texture_image,
                        subresource_range: vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            level_count: 1,
                            layer_count: 1,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    device.cmd_pipeline_barrier(
                        texture_command_buffer,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[texture_barrier_end],
                    );
                },
            );

            // MARK: SAMPLER
            let sampler_info = vk::SamplerCreateInfo {
                mag_filter: vk::Filter::LINEAR,
                min_filter: vk::Filter::LINEAR,
                mipmap_mode: vk::SamplerMipmapMode::LINEAR,
                address_mode_u: vk::SamplerAddressMode::MIRRORED_REPEAT,
                address_mode_v: vk::SamplerAddressMode::MIRRORED_REPEAT,
                address_mode_w: vk::SamplerAddressMode::MIRRORED_REPEAT,
                max_anisotropy: 1.0,
                border_color: vk::BorderColor::FLOAT_OPAQUE_WHITE,
                compare_op: vk::CompareOp::NEVER,
                ..Default::default()
            };

            let texture_sampler = device.create_sampler(&sampler_info, None).unwrap();

            // MARK: TEXTURE VIEW
            let tex_image_view_info = vk::ImageViewCreateInfo {
                view_type: vk::ImageViewType::TYPE_2D,
                format: texture_create_info.format,
                components: vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                },
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    level_count: 1,
                    layer_count: 1,
                    ..Default::default()
                },
                image: texture_image,
                ..Default::default()
            };
            let tex_image_view = device
                .create_image_view(&tex_image_view_info, None)
                .unwrap();

            let uniform_color_buffer_descriptor = vk::DescriptorBufferInfo {
                buffer: uniform_color_buffer,
                offset: 0,
                range: mem::size_of_val(&uniform) as u64,
            };

            let tex_descriptor = vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                image_view: tex_image_view,
                sampler: texture_sampler,
            };

            let write_desc_sets = [
                vk::WriteDescriptorSet {
                    dst_set: descriptor_sets[0],
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    p_buffer_info: &uniform_color_buffer_descriptor,
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: descriptor_sets[0],
                    dst_binding: 1,
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    p_image_info: &tex_descriptor,
                    ..Default::default()
                },
            ];
            device.update_descriptor_sets(&write_desc_sets, &[]);

            // MARK: MESHES
            let mut registered_meshes = Vec::new();

            for _ in 0..5 {
                let mut vertices = Vec::new();
                let mut indices = Vec::new();
                for _ in 0..10 {
                    let x = rng.gen_range(-1.0..1.0);
                    let y = rng.gen_range(-1.0..1.0);

                    vertices.extend(
                        [
                            Vertex {
                                pos: [x, y, 1.0, 1.0],
                                uv: [0.0, 0.0],
                            },
                            Vertex {
                                pos: [x + 0.1, y, 1.0, 1.0],
                                uv: [0.0, 1.0],
                            },
                            Vertex {
                                pos: [x + 0.1, y - 0.1, 1.0, 1.0],
                                uv: [1.0, 1.0],
                            },
                            Vertex {
                                pos: [x, y - 0.1, 1.0, 1.0],
                                uv: [1.0, 0.0],
                            },
                        ]
                        .iter(),
                    );

                    let offset = vertices.len() as u32 - 4;
                    let quad_indices = vec![
                        offset + 0,
                        offset + 1,
                        offset + 2,
                        offset + 0,
                        offset + 2,
                        offset + 3,
                    ];

                    indices.extend(quad_indices);
                }
                let mesh = Mesh { vertices, indices };
                let registered_mesh = mesh.register(&device, &device_memory_properties);
                registered_meshes.push(registered_mesh);
            }

            // MARK: SQUARE
            let square = Mesh {
                vertices: vec![
                    Vertex {
                        pos: [-1.0, -1.0, 0.0, 1.0],
                        uv: [0.0, 0.0],
                    },
                    Vertex {
                        pos: [-1.0, 1.0, 0.0, 1.0],
                        uv: [0.0, 1.0],
                    },
                    Vertex {
                        pos: [1.0, 1.0, 0.0, 1.0],
                        uv: [1.0, 1.0],
                    },
                    Vertex {
                        pos: [1.0, -1.0, 0.0, 1.0],
                        uv: [1.0, 0.0],
                    },
                ],
                indices: vec![0u32, 1, 2, 2, 3, 0],
            };
            let registered_square = square.register(&device, &device_memory_properties);
            registered_meshes.push(registered_square);

            let swapchain_resources = SwapchainResources {
                pool,
                draw_command_buffer,
                setup_command_buffer,
                depth_image,
                depth_image_view,
                depth_image_memory,
                draw_commands_reuse_fence,
                setup_commands_reuse_fence,
                present_complete_semaphore,
                rendering_complete_semaphore,
                present_images,
                present_image_views,
            };

            Ok((
                Self {
                    rng,

                    window,
                    entry,

                    instance,
                    pdevices,
                    surface_loader,

                    debug_utils_loader,
                    debug_call_back,

                    device_memory_properties,
                    queue_family_index,
                    pdevice,

                    surface,

                    swapchain_loader,
                    swapchain,
                    swapchain_resources,

                    device,

                    renderpass,
                    framebuffers,
                    graphic_pipeline,

                    viewports,
                    scissors,
                    graphics_pipelines,
                    pipeline_layout,
                    vertex_shader_module,
                    fragment_shader_module,

                    descriptor_sets,
                    desc_set_layouts,
                    image_buffer_memory,
                    image_buffer,
                    texture_memory,
                    tex_image_view,
                    texture_image,
                    uniform_color_buffer,
                    uniform_buffer_memory: uniform_color_buffer_memory,
                    descriptor_pool,
                    texture_sampler,

                    registered_meshes,

                    minimized: false,

                    metrics: Metrics::default(),

                    uniform,
                },
                event_loop,
            ))
        }
    }

    unsafe fn create_instance(
        entry: &Entry,
        window: &winit::window::Window,
    ) -> Result<Instance, Box<dyn Error>> {
        let app_name = ffi::CStr::from_bytes_with_nul_unchecked(APP_NAME.as_bytes());
        let appinfo = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 0, 0));
        let mut extension_names =
            ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())
                .unwrap()
                .to_vec();
        extension_names.push(debug_utils::NAME.as_ptr());
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            // Enabling this extension is a requirement when using `VK_KHR_portability_subset`
            extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
        }
        let layer_names = [ffi::CStr::from_bytes_with_nul_unchecked(
            b"VK_LAYER_KHRONOS_validation\0",
        )];
        let layers_names_raw: Vec<*const c_char> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();
        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&appinfo)
            .enabled_layer_names(&layers_names_raw)
            .enabled_extension_names(&extension_names)
            .flags(create_flags);
        let instance: Instance = entry
            .create_instance(&create_info, None)
            .expect("Instance creation error");

        Ok(instance)
    }

    unsafe fn create_debug_utils_messenger(
        entry: &Entry,
        instance: &Instance,
    ) -> Result<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT), Box<dyn Error>> {
        let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));
        let debug_utils_loader = debug_utils::Instance::new(&entry, &instance);
        let debug_call_back = debug_utils_loader
            .create_debug_utils_messenger(&debug_info, None)
            .unwrap();

        Ok((debug_utils_loader, debug_call_back))
    }

    unsafe fn create_surface(
        entry: &Entry,
        instance: &Instance,
        window: &winit::window::Window,
        pdevices: &Vec<ash::vk::PhysicalDevice>,
        surface_loader: &surface::Instance,
    ) -> Result<(EngineSurface, vk::PhysicalDevice, u32), Box<dyn Error>> {
        let surface = ash_window::create_surface(
            &entry,
            &instance,
            window.display_handle()?.as_raw(),
            window.window_handle()?.as_raw(),
            None,
        )
        .unwrap();

        let (pdevice, queue_family_index) = pdevices
            .iter()
            .find_map(|pdevice| {
                instance
                    .get_physical_device_queue_family_properties(*pdevice)
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        let supports_graphic_and_surface =
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                && surface_loader
                                    .get_physical_device_surface_support(
                                        *pdevice,
                                        index as u32,
                                        surface,
                                    )
                                    .unwrap();
                        if supports_graphic_and_surface {
                            Some((*pdevice, index))
                        } else {
                            None
                        }
                    })
            })
            .expect("Couldn't find suitable device.");
        let queue_family_index = queue_family_index as u32;

        let surface_format = surface_loader
            .get_physical_device_surface_formats(pdevice, surface)
            .unwrap()[0];

        let surface_capabilities = surface_loader
            .get_physical_device_surface_capabilities(pdevice, surface)
            .unwrap();

        Ok((
            EngineSurface {
                surface_khr: surface,
                format: surface_format,
                capabilities: surface_capabilities,
                resolution: surface_capabilities.current_extent,
            },
            pdevice,
            queue_family_index,
        ))
    }

    unsafe fn create_device(
        instance: &Instance,
        pdevice: vk::PhysicalDevice,
        queue_family_index: u32,
    ) -> Result<Device, Box<dyn Error>> {
        let priorities = [1.0];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities);
        let device_extension_names_raw = [
            swapchain::NAME.as_ptr(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME.as_ptr(),
        ];
        let features = vk::PhysicalDeviceFeatures {
            shader_clip_distance: 1,
            ..Default::default()
        };
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);
        let device: Device = instance
            .create_device(pdevice, &device_create_info, None)
            .unwrap();

        Ok(device)
    }

    unsafe fn create_swapchain(
        device: &Device,
        surface_loader: &surface::Instance,
        surface: &EngineSurface,
        pdevice: vk::PhysicalDevice,
        queue_family_index: u32,
        window_inner_size: PhysicalSize<u32>,
        swapchain_loader: &swapchain::Device,
    ) -> Result<EngineSwapchain, Box<dyn Error>> {
        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(pdevice, surface.surface_khr)
            .unwrap();
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let present_queue = device.get_device_queue(queue_family_index, 0);

        let mut desired_image_count = surface.capabilities.min_image_count + 1;
        if surface.capabilities.max_image_count > 0
            && desired_image_count > surface.capabilities.max_image_count
        {
            desired_image_count = surface.capabilities.max_image_count;
        }
        let surface_resolution = match surface.capabilities.current_extent.width {
            u32::MAX => vk::Extent2D {
                width: window_inner_size.width,
                height: window_inner_size.height,
            },
            _ => surface.capabilities.current_extent,
        };
        let pre_transform = if surface
            .capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface.capabilities.current_transform
        };

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.surface_khr)
            .min_image_count(desired_image_count)
            .image_color_space(surface.format.color_space)
            .image_format(surface.format.format)
            .image_extent(surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);

        let swapchain = swapchain_loader
            .create_swapchain(&swapchain_create_info, None)
            .unwrap();

        Ok(EngineSwapchain {
            swapchain_khr: swapchain,
            desired_image_count,
            present_mode,
            present_queue,
        })
    }

    unsafe fn create_renderpass(
        surface: &EngineSurface,
        device: &Device,
    ) -> Result<vk::RenderPass, Box<dyn Error>> {
        let renderpass_attachments = [
            vk::AttachmentDescription {
                format: surface.format.format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
            },
            vk::AttachmentDescription {
                format: vk::Format::D16_UNORM,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                initial_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                ..Default::default()
            },
        ];
        let color_attachment_refs = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_attachment_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };
        let dependencies = [vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ..Default::default()
        }];

        let subpass = vk::SubpassDescription::default()
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

        let renderpass_create_info = vk::RenderPassCreateInfo::default()
            .attachments(&renderpass_attachments)
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(&dependencies);

        let renderpass = device
            .create_render_pass(&renderpass_create_info, None)
            .unwrap();

        Ok(renderpass)
    }

    unsafe fn create_fences(device: &Device) -> Result<(vk::Fence, vk::Fence), Box<dyn Error>> {
        let fence_create_info =
            vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        let draw_commands_reuse_fence = device
            .create_fence(&fence_create_info, None)
            .expect("Create fence failed.");
        let setup_commands_reuse_fence = device
            .create_fence(&fence_create_info, None)
            .expect("Create fence failed.");

        Ok((draw_commands_reuse_fence, setup_commands_reuse_fence))
    }

    unsafe fn create_views_and_depth(
        device: &Device,
        instance: &Instance,
        swapchain: &EngineSwapchain,
        surface: &EngineSurface,
        pdevice: &vk::PhysicalDevice,
        swapchain_loader: &swapchain::Device,
    ) -> Result<
        (
            Vec<ash::vk::Image>,
            Vec<vk::ImageView>,
            vk::ImageView,
            vk::Image,
            vk::DeviceMemory,
            vk::PhysicalDeviceMemoryProperties,
        ),
        Box<dyn Error>,
    > {
        let present_images = swapchain_loader
            .get_swapchain_images(swapchain.swapchain_khr)
            .unwrap();
        let present_image_views: Vec<vk::ImageView> = present_images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::default()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface.format.format)
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image(image);
                device.create_image_view(&create_view_info, None).unwrap()
            })
            .collect();
        let device_memory_properties = instance.get_physical_device_memory_properties(*pdevice);
        let depth_image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::D16_UNORM)
            .extent(surface.resolution.into())
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let depth_image = device.create_image(&depth_image_create_info, None).unwrap();
        let depth_image_memory_req = device.get_image_memory_requirements(depth_image);
        let depth_image_memory_index = find_memorytype_index(
            &depth_image_memory_req,
            &device_memory_properties,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .expect("Unable to find suitable memory index for depth image.");

        let depth_image_allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(depth_image_memory_req.size)
            .memory_type_index(depth_image_memory_index);

        let depth_image_memory = device
            .allocate_memory(&depth_image_allocate_info, None)
            .unwrap();

        device
            .bind_image_memory(depth_image, depth_image_memory, 0)
            .expect("Unable to bind depth image memory");

        let depth_image_view_info = vk::ImageViewCreateInfo::default()
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::DEPTH)
                    .level_count(1)
                    .layer_count(1),
            )
            .image(depth_image)
            .format(depth_image_create_info.format)
            .view_type(vk::ImageViewType::TYPE_2D);

        let depth_image_view = device
            .create_image_view(&depth_image_view_info, None)
            .unwrap();

        Ok((
            present_images,
            present_image_views,
            depth_image_view,
            depth_image,
            depth_image_memory,
            device_memory_properties,
        ))
    }

    unsafe fn create_semaphores(
        device: &Device,
    ) -> Result<(vk::Semaphore, vk::Semaphore), Box<dyn Error>> {
        let semaphore_create_info = vk::SemaphoreCreateInfo::default();

        let present_complete_semaphore = device
            .create_semaphore(&semaphore_create_info, None)
            .unwrap();
        let rendering_complete_semaphore = device
            .create_semaphore(&semaphore_create_info, None)
            .unwrap();

        Ok((present_complete_semaphore, rendering_complete_semaphore))
    }

    unsafe fn create_pipeline(
        device: &Device,
        surface: &EngineSurface,
        renderpass: vk::RenderPass,
        desc_set_layouts: [vk::DescriptorSetLayout; 1],
    ) -> Result<
        (
            vk::Pipeline,
            [vk::Viewport; 1],
            [vk::Rect2D; 1],
            Vec<vk::Pipeline>,
            vk::PipelineLayout,
            vk::ShaderModule,
            vk::ShaderModule,
        ),
        Box<dyn Error>,
    > {
        let vertex_shader = Shader::from_filename("vert", vk::ShaderStageFlags::VERTEX, device);
        let frag_shader = Shader::from_filename("frag", vk::ShaderStageFlags::FRAGMENT, device);

        let shader_stage_create_infos = [
            vertex_shader.pipeline_shader_stage_create_info,
            frag_shader.pipeline_shader_stage_create_info,
        ];

        let layout_create_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(&desc_set_layouts);
        let pipeline_layout = device
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap();

        let vertex_input_binding_descriptions = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: mem::size_of::<Vertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let vertex_input_attribute_descriptions = [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: offset_of!(Vertex, pos) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(Vertex, uv) as u32,
            },
        ];

        let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&vertex_input_attribute_descriptions)
            .vertex_binding_descriptions(&vertex_input_binding_descriptions);
        let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            ..Default::default()
        };

        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: surface.resolution.width as f32,
            height: surface.resolution.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = [surface.resolution.into()];
        let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
            .scissors(&scissors)
            .viewports(&viewports);

        let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            line_width: 1.0,
            polygon_mode: vk::PolygonMode::FILL,
            ..Default::default()
        };
        let multisample_state_info = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let noop_stencil_state = vk::StencilOpState {
            fail_op: vk::StencilOp::KEEP,
            pass_op: vk::StencilOp::KEEP,
            depth_fail_op: vk::StencilOp::KEEP,
            compare_op: vk::CompareOp::ALWAYS,
            ..Default::default()
        };
        let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: 1,
            depth_write_enable: 1,
            depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
            front: noop_stencil_state,
            back: noop_stencil_state,
            max_depth_bounds: 1.0,
            ..Default::default()
        };
        let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
            blend_enable: 0,
            src_color_blend_factor: vk::BlendFactor::SRC_COLOR,
            dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_COLOR,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ZERO,
            dst_alpha_blend_factor: vk::BlendFactor::ZERO,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        }];
        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op(vk::LogicOp::CLEAR)
            .attachments(&color_blend_attachment_states);

        let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_state);

        let graphic_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stage_create_infos)
            .vertex_input_state(&vertex_input_state_info)
            .input_assembly_state(&vertex_input_assembly_state_info)
            .viewport_state(&viewport_state_info)
            .rasterization_state(&rasterization_info)
            .multisample_state(&multisample_state_info)
            .depth_stencil_state(&depth_state_info)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state_info)
            .layout(pipeline_layout)
            .render_pass(renderpass);

        let graphics_pipelines = device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[graphic_pipeline_info], None)
            .expect("Unable to create graphics pipeline");

        let graphic_pipeline = graphics_pipelines[0];

        Ok((
            graphic_pipeline,
            viewports,
            scissors,
            graphics_pipelines,
            pipeline_layout,
            vertex_shader.module,
            frag_shader.module,
        ))
    }

    unsafe fn create_framebuffers(
        device: &Device,
        surface: &EngineSurface,
        present_image_views: &Vec<vk::ImageView>,
        depth_image_view: vk::ImageView,
        renderpass: vk::RenderPass,
    ) -> Result<Vec<vk::Framebuffer>, Box<dyn Error>> {
        let framebuffers: Vec<vk::Framebuffer> = present_image_views
            .iter()
            .map(|&present_image_view| {
                let framebuffer_attachments = [present_image_view, depth_image_view];
                let frame_buffer_create_info = vk::FramebufferCreateInfo::default()
                    .render_pass(renderpass)
                    .attachments(&framebuffer_attachments)
                    .width(surface.resolution.width)
                    .height(surface.resolution.height)
                    .layers(1);

                device
                    .create_framebuffer(&frame_buffer_create_info, None)
                    .unwrap()
            })
            .collect();

        Ok(framebuffers)
    }

    unsafe fn create_command_pools(
        device: &Device,
        queue_family_index: u32,
    ) -> Result<vk::CommandPool, Box<dyn Error>> {
        let pool_create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);

        let pool = device.create_command_pool(&pool_create_info, None).unwrap();

        Ok(pool)
    }

    unsafe fn create_command_buffers(
        device: &Device,
        pool: vk::CommandPool,
    ) -> Result<(vk::CommandBuffer, vk::CommandBuffer), Box<dyn Error>> {
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_buffer_count(2)
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers = device
            .allocate_command_buffers(&command_buffer_allocate_info)
            .unwrap();
        let setup_command_buffer = command_buffers[0];
        let draw_command_buffer = command_buffers[1];

        Ok((setup_command_buffer, draw_command_buffer))
    }

    unsafe fn create_uniform_buffer(
        device: &Device,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        uniform: Mat4,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let uniform_buffer_info = vk::BufferCreateInfo {
            size: mem::size_of_val(&uniform) as u64,
            usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let uniform_buffer = device.create_buffer(&uniform_buffer_info, None).unwrap();
        let uniform_buffer_memory_req = device.get_buffer_memory_requirements(uniform_buffer);
        let uniform_buffer_memory_index = find_memorytype_index(
            &uniform_buffer_memory_req,
            &device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the vertex buffer.");

        let uniform_buffer_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: uniform_buffer_memory_req.size,
            memory_type_index: uniform_buffer_memory_index,
            ..Default::default()
        };
        let uniform_buffer_memory = device
            .allocate_memory(&uniform_buffer_allocate_info, None)
            .unwrap();
        let uniform_ptr = device
            .map_memory(
                uniform_buffer_memory,
                0,
                uniform_buffer_memory_req.size,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let mut uniform_aligned_slice = Align::new(
            uniform_ptr,
            mem::align_of::<Mat4>() as u64,
            uniform_buffer_memory_req.size,
        );
        uniform_aligned_slice.copy_from_slice(&[uniform]);
        device.unmap_memory(uniform_buffer_memory);
        device
            .bind_buffer_memory(uniform_buffer, uniform_buffer_memory, 0)
            .unwrap();

        (uniform_buffer, uniform_buffer_memory)
    }

    unsafe fn update_uniform_buffer(
        device: &Device,
        uniform_buffer_memory: vk::DeviceMemory,
        new_transform: Mat4,
    ) {
        let uniform_ptr = device
            .map_memory(
                uniform_buffer_memory,
                0,
                mem::size_of::<Mat4>() as u64,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();

        let mut uniform_aligned_slice = Align::new(
            uniform_ptr,
            mem::align_of::<Mat4>() as u64,
            mem::size_of::<Mat4>() as u64,
        );

        uniform_aligned_slice.copy_from_slice(&[new_transform]);
        device.unmap_memory(uniform_buffer_memory);
    }

    pub fn render(&mut self) {
        self.metrics.start_frame();
        let delta = self.metrics.delta_start_to_start;

        if !self.minimized {
            return;
        }

        self.uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, delta.as_secs_f32());

        unsafe {
            Engine::update_uniform_buffer(&self.device, self.uniform_buffer_memory, self.uniform);

            let result = self.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain_khr,
                u64::MAX,
                self.swapchain_resources.present_complete_semaphore,
                vk::Fence::null(),
            );
            let (present_index, _) = match result {
                Ok(result) => result,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    eprintln!("OUT_OF_DATE_KHR caught at start");
                    return;
                }
                Err(err) => panic!("Failed to acquire next image: {:?}", err),
            };
            let clear_values = [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ];

            let render_pass_begin_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.renderpass)
                .framebuffer(self.framebuffers[present_index as usize])
                .render_area(self.surface.resolution.into())
                .clear_values(&clear_values);

            record_submit_commandbuffer(
                &self.device,
                self.swapchain_resources.draw_command_buffer,
                self.swapchain_resources.draw_commands_reuse_fence,
                self.swapchain.present_queue,
                &[vk::PipelineStageFlags::BOTTOM_OF_PIPE],
                &[self.swapchain_resources.present_complete_semaphore],
                &[self.swapchain_resources.rendering_complete_semaphore],
                |device, draw_command_buffer| {
                    device.cmd_begin_render_pass(
                        draw_command_buffer,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    device.cmd_bind_descriptor_sets(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_layout,
                        0,
                        &self.descriptor_sets,
                        &[],
                    );
                    device.cmd_bind_pipeline(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.graphic_pipeline,
                    );
                    device.cmd_set_viewport(draw_command_buffer, 0, &self.viewports);
                    device.cmd_set_scissor(draw_command_buffer, 0, &self.scissors);

                    for registered_mesh in &self.registered_meshes {
                        device.cmd_bind_vertex_buffers(
                            draw_command_buffer,
                            0,
                            &[registered_mesh.vertex_buffer],
                            &[0],
                        );
                        device.cmd_bind_index_buffer(
                            draw_command_buffer,
                            registered_mesh.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_draw_indexed(
                            draw_command_buffer,
                            registered_mesh.mesh.indices.len() as u32,
                            1,
                            0,
                            0,
                            1,
                        );
                    }

                    // Or draw without the index buffer
                    // device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                    device.cmd_end_render_pass(draw_command_buffer);
                },
            );
            let wait_semaphors = [self.swapchain_resources.rendering_complete_semaphore];
            let swapchains = [self.swapchain.swapchain_khr];
            let image_indices = [present_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&wait_semaphors)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let queue_present_result = self
                .swapchain_loader
                .queue_present(self.swapchain.present_queue, &present_info);

            match queue_present_result {
                Ok(_) => {}
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => eprintln!("OUT_OF_DATE_KHR at present"),
                Err(err) => panic!("Failed to present queue: {:?}", err),
            }
        }

        self.metrics.end_frame();
    }

    pub fn start_update_thread(&self) -> JoinHandle<()> {
        std::thread::spawn(move || {
            // IMPLEMENTATION
        })
    }

    pub fn recreate_swapchain(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            self.minimized = false;
            return;
        }

        self.minimized = true;

        if size.width == self.surface.resolution.width
            && size.height == self.surface.resolution.height
        {
            return;
        }

        unsafe {
            self.device.device_wait_idle().unwrap();

            self.destroy_swapchain();

            // surface
            self.surface.resolution = vk::Extent2D {
                width: size.width,
                height: size.height,
            };
            self.surface.capabilities = self
                .surface_loader
                .get_physical_device_surface_capabilities(self.pdevice, self.surface.surface_khr)
                .unwrap();

            // viewports and scissors
            self.viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: size.width as f32,
                height: size.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: size.width,
                    height: size.height,
                },
            }];

            // swapchain
            self.swapchain = Self::create_swapchain(
                &self.device,
                &self.surface_loader,
                &self.surface,
                self.pdevice,
                self.queue_family_index,
                size,
                &self.swapchain_loader,
            )
            .unwrap();

            // image_views
            let (
                present_images,
                present_image_views,
                depth_image_view,
                depth_image,
                depth_image_memory,
                device_memory_properties,
            ) = Self::create_views_and_depth(
                &self.device,
                &self.instance,
                &self.swapchain,
                &self.surface,
                &self.pdevice,
                &self.swapchain_loader,
            )
            .unwrap();

            self.swapchain_resources.present_images = present_images;
            self.swapchain_resources.present_image_views = present_image_views;
            self.swapchain_resources.depth_image_view = depth_image_view;
            self.swapchain_resources.depth_image = depth_image;
            self.swapchain_resources.depth_image_memory = depth_image_memory;
            self.device_memory_properties = device_memory_properties;

            // framebuffers
            let framebuffers = Self::create_framebuffers(
                &self.device,
                &self.surface,
                &self.swapchain_resources.present_image_views,
                depth_image_view,
                self.renderpass,
            )
            .unwrap();

            self.framebuffers = framebuffers;

            // register depth image memory
            record_submit_commandbuffer(
                &self.device,
                self.swapchain_resources.setup_command_buffer,
                self.swapchain_resources.setup_commands_reuse_fence,
                self.swapchain.present_queue,
                &[],
                &[],
                &[],
                |device, setup_command_buffer| {
                    let layout_transition_barriers = vk::ImageMemoryBarrier::default()
                        .image(depth_image)
                        .dst_access_mask(
                            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        )
                        .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                                .layer_count(1)
                                .level_count(1),
                        );

                    device.cmd_pipeline_barrier(
                        setup_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[layout_transition_barriers],
                    );
                },
            );

            self.render();
        }
    }

    unsafe fn destroy_swapchain(&mut self) {
        for &framebuffer in self.framebuffers.iter() {
            self.device.destroy_framebuffer(framebuffer, None);
        }
        for &image_view in self.swapchain_resources.present_image_views.iter() {
            self.device.destroy_image_view(image_view, None);
        }
        self.swapchain_loader
            .destroy_swapchain(self.swapchain.swapchain_khr, None);

        self.device
            .free_memory(self.swapchain_resources.depth_image_memory, None);
        self.device
            .destroy_image_view(self.swapchain_resources.depth_image_view, None);
        self.device
            .destroy_image(self.swapchain_resources.depth_image, None);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.device
                .destroy_shader_module(self.vertex_shader_module, None);
            self.device
                .destroy_shader_module(self.fragment_shader_module, None);

            self.device.free_memory(self.image_buffer_memory, None);
            self.device.destroy_buffer(self.image_buffer, None);
            self.device.free_memory(self.texture_memory, None);
            self.device.destroy_image_view(self.tex_image_view, None);
            self.device.destroy_image(self.texture_image, None);

            for registered_mesh in self.registered_meshes.iter() {
                self.device
                    .free_memory(registered_mesh.index_buffer_memory, None);
                self.device
                    .destroy_buffer(registered_mesh.index_buffer, None);
                self.device
                    .free_memory(registered_mesh.vertex_buffer_memory, None);
                self.device
                    .destroy_buffer(registered_mesh.vertex_buffer, None);
            }

            for &descriptor_set_layout in self.desc_set_layouts.iter() {
                self.device
                    .destroy_descriptor_set_layout(descriptor_set_layout, None);
            }
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_sampler(self.texture_sampler, None);

            self.device.free_memory(self.uniform_buffer_memory, None);
            self.device.destroy_buffer(self.uniform_color_buffer, None);

            self.destroy_swapchain();

            for &pipeline in self.graphics_pipelines.iter() {
                self.device.destroy_pipeline(pipeline, None);
            }
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);

            self.device.destroy_render_pass(self.renderpass, None);

            self.device
                .destroy_semaphore(self.swapchain_resources.present_complete_semaphore, None);
            self.device
                .destroy_semaphore(self.swapchain_resources.rendering_complete_semaphore, None);

            self.device
                .destroy_fence(self.swapchain_resources.draw_commands_reuse_fence, None);
            self.device
                .destroy_fence(self.swapchain_resources.setup_commands_reuse_fence, None);

            self.device
                .destroy_command_pool(self.swapchain_resources.pool, None);

            self.device.destroy_device(None);

            self.debug_utils_loader
                .destroy_debug_utils_messenger(self.debug_call_back, None);

            self.surface_loader
                .destroy_surface(self.surface.surface_khr, None);

            self.instance.destroy_instance(None);
        }
    }
}
