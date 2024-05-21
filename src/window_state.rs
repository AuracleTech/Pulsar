use crate::{
    app::Application,
    input_manager::EventStates,
    model::{Mesh, Vertex},
    vulkan::{
        record::record_submit_commandbuffer,
        surface::{AAAResources, AAASurface},
        views::find_memorytype_index,
    },
};
use ash::{
    khr::{surface, swapchain},
    util::Align,
    vk,
};
use cursor_icon::CursorIcon;
use glam::Mat4;
use log::info;
use rand::Rng;
use std::{
    error::Error,
    mem,
    sync::{Arc, Mutex},
    thread,
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    keyboard::ModifiersState,
    window::{Cursor, CursorGrabMode, CustomCursor, Fullscreen, ResizeDirection, Theme, Window},
};

/// The amount of points to around the window for drag resize direction calculations.
const BORDER_SIZE: f64 = 20.;

const CURSORS: &[CursorIcon] = &[
    CursorIcon::Default,
    CursorIcon::Crosshair,
    CursorIcon::Pointer,
    CursorIcon::Move,
    CursorIcon::Text,
    CursorIcon::Wait,
    CursorIcon::Help,
    CursorIcon::Progress,
    CursorIcon::NotAllowed,
    CursorIcon::ContextMenu,
    CursorIcon::Cell,
    CursorIcon::VerticalText,
    CursorIcon::Alias,
    CursorIcon::Copy,
    CursorIcon::NoDrop,
    CursorIcon::Grab,
    CursorIcon::Grabbing,
    CursorIcon::AllScroll,
    CursorIcon::ZoomIn,
    CursorIcon::ZoomOut,
    CursorIcon::EResize,
    CursorIcon::NResize,
    CursorIcon::NeResize,
    CursorIcon::NwResize,
    CursorIcon::SResize,
    CursorIcon::SeResize,
    CursorIcon::SwResize,
    CursorIcon::WResize,
    CursorIcon::EwResize,
    CursorIcon::NsResize,
    CursorIcon::NeswResize,
    CursorIcon::NwseResize,
    CursorIcon::ColResize,
    CursorIcon::RowResize,
];

pub struct WindowState {
    /// IME input.
    ime: bool,
    /// The actual winit Window.
    pub window: Arc<Window>,
    /// The window theme we're drawing with.
    theme: Theme,
    /// Cursor position over the window.
    cursor_position: Option<PhysicalPosition<f64>>,
    /// Window modifiers state.
    pub modifiers: ModifiersState,
    /// Occlusion state of the window.
    occluded: bool,
    /// Current cursor grab mode.
    cursor_grab: CursorGrabMode,
    /// The amount of zoom into window.
    pub zoom: f64,
    /// The amount of rotation of the window.
    pub rotated: f32,
    /// The amount of pan of the window.
    pub panned: PhysicalPosition<f32>,

    #[cfg(macos_platform)]
    option_as_alt: OptionAsAlt,

    // Cursor states.
    named_idx: usize,
    custom_idx: usize,
    cursor_hidden: bool,

    // Render
    pub surface: Arc<Mutex<AAASurface>>, // TODO Remove pub
    pub render_handle: Option<thread::JoinHandle<()>>,
    pub event_states: Arc<EventStates>,
}

impl WindowState {
    pub fn new(app: &Application, window: Window) -> Result<Self, Box<dyn Error>> {
        let window = Arc::new(window);

        let theme = window.theme().unwrap_or(Theme::Dark);
        info!("Theme: {theme:?}");
        let named_idx = 0;
        window.set_cursor(CURSORS[named_idx]);

        // Allow IME out of the box.
        let ime = true;
        window.set_ime_allowed(ime);

        let surface = crate::vulkan::surface::AAASurface::new(
            &app.entry,
            &app.instance,
            &window,
            &app.physical_device_list,
            &app.surface_loader,
        )
        .unwrap();

        Ok(Self {
            #[cfg(macos_platform)]
            option_as_alt: window.option_as_alt(),
            custom_idx: app.custom_cursors.len() - 1,
            cursor_grab: CursorGrabMode::None,
            named_idx,
            window,
            theme,
            ime,
            cursor_position: Default::default(),
            cursor_hidden: Default::default(),
            modifiers: Default::default(),
            occluded: Default::default(),
            rotated: Default::default(),
            panned: Default::default(),
            zoom: Default::default(),
            render_handle: Default::default(),
            surface: Arc::new(Mutex::new(surface)),
            event_states: Arc::new(Default::default()),
        })
    }

    pub fn toggle_ime(&mut self) {
        self.ime = !self.ime;
        self.window.set_ime_allowed(self.ime);
        if let Some(position) = self.ime.then_some(self.cursor_position).flatten() {
            self.window
                .set_ime_cursor_area(position, PhysicalSize::new(20, 20));
        }
    }

    pub fn minimize(&mut self) {
        self.window.set_minimized(true);
    }

    pub fn cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_position = Some(position);
        if self.ime {
            self.window
                .set_ime_cursor_area(position, PhysicalSize::new(20, 20));
        }
    }

    pub fn cursor_left(&mut self) {
        self.cursor_position = None;
    }

    /// Toggle maximized.
    pub fn toggle_maximize(&self) {
        let maximized = self.window.is_maximized();
        self.window.set_maximized(!maximized);
    }

    /// Toggle window decorations.
    pub fn toggle_decorations(&self) {
        let decorated = self.window.is_decorated();
        self.window.set_decorations(!decorated);
    }

    /// Toggle window resizable state.
    pub fn toggle_resizable(&self) {
        let resizable = self.window.is_resizable();
        self.window.set_resizable(!resizable);
    }

    /// Toggle cursor visibility
    pub fn toggle_cursor_visibility(&mut self) {
        self.cursor_hidden = !self.cursor_hidden;
        self.window.set_cursor_visible(!self.cursor_hidden);
    }

    /// Toggle resize increments on a window.
    pub fn toggle_resize_increments(&mut self) {
        let new_increments = match self.window.resize_increments() {
            Some(_) => None,
            None => Some(LogicalSize::new(25.0, 25.0)),
        };
        // info!("Had increments: {}", new_increments.is_none());
        self.window.set_resize_increments(new_increments);
    }

    /// Toggle fullscreen.
    pub fn toggle_fullscreen(&self) {
        let fullscreen = if self.window.fullscreen().is_some() {
            None
        } else {
            Some(Fullscreen::Borderless(None))
        };

        self.window.set_fullscreen(fullscreen);
    }

    /// Cycle through the grab modes ignoring errors.
    pub fn cycle_cursor_grab(&mut self) {
        self.cursor_grab = match self.cursor_grab {
            // CursorGrabMode::Locked is unimplemented yet.
            CursorGrabMode::None => CursorGrabMode::Confined,
            CursorGrabMode::Confined => CursorGrabMode::None,
            _ => CursorGrabMode::None,
        };
        info!("Changing cursor grab mode to {:?}", self.cursor_grab);
        if let Err(err) = self.window.set_cursor_grab(self.cursor_grab) {
            panic!("Error setting cursor grab: {err}");
        }
    }

    #[cfg(macos_platform)]
    fn cycle_option_as_alt(&mut self) {
        self.option_as_alt = match self.option_as_alt {
            OptionAsAlt::None => OptionAsAlt::OnlyLeft,
            OptionAsAlt::OnlyLeft => OptionAsAlt::OnlyRight,
            OptionAsAlt::OnlyRight => OptionAsAlt::Both,
            OptionAsAlt::Both => OptionAsAlt::None,
        };
        // info!("Setting option as alt {:?}", self.option_as_alt);
        self.window.set_option_as_alt(self.option_as_alt);
    }

    /// Swap the window dimensions with `request_inner_size`.
    pub fn swap_dimensions(&mut self) {
        let old_inner_size = self.window.inner_size();
        let mut inner_size = old_inner_size;

        mem::swap(&mut inner_size.width, &mut inner_size.height);
        // info!("Requesting resize from {old_inner_size:?} to {inner_size:?}");

        if let Some(new_inner_size) = self.window.request_inner_size(inner_size) {
            if old_inner_size == new_inner_size {
                // info!("Inner size change got ignored");
            } else {
                self.resize(new_inner_size);
            }
        } else {
            // info!("Request inner size is asynchronous");
        }
    }

    pub fn next_cursor(&mut self) {
        self.named_idx = (self.named_idx + 1) % CURSORS.len();
        // info!("Setting cursor to \"{:?}\"", CURSORS[self.named_idx]);
        self.window
            .set_cursor(Cursor::Icon(CURSORS[self.named_idx]));
    }

    pub fn next_custom_cursor(&mut self, custom_cursors: &[CustomCursor]) {
        self.custom_idx = (self.custom_idx + 1) % custom_cursors.len();
        let cursor = Cursor::Custom(custom_cursors[self.custom_idx].clone());
        self.window.set_cursor(cursor);
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        #[cfg(not(any(android_platform, ios_platform)))]
        {
            self.event_states.resize(size);
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn show_menu(&self) {
        if let Some(position) = self.cursor_position {
            self.window.show_window_menu(position);
        }
    }

    pub fn drag_window(&self) {
        if let Err(err) = self.window.drag_window() {
            info!("Error starting window drag: {err}");
        } else {
            info!("Dragging window Window={:?}", self.window.id());
        }
    }

    pub fn drag_resize_window(&self) {
        let position = match self.cursor_position {
            Some(position) => position,
            None => {
                info!("Drag-resize requires cursor to be inside the window");
                return;
            }
        };

        let win_size = self.window.inner_size();
        let border_size = BORDER_SIZE * self.window.scale_factor();

        let x_direction = if position.x < border_size {
            ResizeDirection::West
        } else if position.x > (win_size.width as f64 - border_size) {
            ResizeDirection::East
        } else {
            // Use arbitrary direction instead of None for simplicity.
            ResizeDirection::SouthEast
        };

        let y_direction = if position.y < border_size {
            ResizeDirection::North
        } else if position.y > (win_size.height as f64 - border_size) {
            ResizeDirection::South
        } else {
            // Use arbitrary direction instead of None for simplicity.
            ResizeDirection::SouthEast
        };

        let direction = match (x_direction, y_direction) {
            (ResizeDirection::West, ResizeDirection::North) => ResizeDirection::NorthWest,
            (ResizeDirection::West, ResizeDirection::South) => ResizeDirection::SouthWest,
            (ResizeDirection::West, _) => ResizeDirection::West,
            (ResizeDirection::East, ResizeDirection::North) => ResizeDirection::NorthEast,
            (ResizeDirection::East, ResizeDirection::South) => ResizeDirection::SouthEast,
            (ResizeDirection::East, _) => ResizeDirection::East,
            (_, ResizeDirection::South) => ResizeDirection::South,
            (_, ResizeDirection::North) => ResizeDirection::North,
            _ => return,
        };

        if let Err(err) = self.window.drag_resize_window(direction) {
            info!("Error starting window drag-resize: {err}");
        } else {
            info!("Drag-resizing window Window={:?}", self.window.id());
        }
    }

    /// Change window occlusion state.
    pub fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
        if occluded {
            // TODO stop rendering
        }
    }

    pub fn init(&mut self, instance: Arc<ash::Instance>, surface_loader: Arc<surface::Instance>) {
        let surface = self.surface.lock().unwrap();

        let mut device = crate::vulkan::device::create_device(
            &instance,
            surface.physical_device,
            surface.queue_family_index,
        )
        .unwrap();

        let swapchain_loader = swapchain::Device::new(&instance, &device);

        // TODO get from os window api for linux and possibly more
        let size = surface.capabilities.current_extent;

        let swapchain = crate::vulkan::swapchain::AAASwapchain::new(
            &device,
            &surface_loader,
            &surface,
            surface.physical_device,
            surface.queue_family_index,
            size.width,
            size.height,
            &swapchain_loader,
        )
        .unwrap();

        let (draw_commands_reuse_fence, setup_commands_reuse_fence) =
            crate::vulkan::fence_semaphores::create_fences(&device).unwrap();

        let (
            present_images,
            present_image_views,
            depth_image_view,
            depth_image,
            depth_image_memory,
            mut device_memory_properties,
        ) = crate::vulkan::views::create_views_and_depth(
            &device,
            &instance,
            &swapchain,
            &surface,
            &surface.physical_device,
            &swapchain_loader,
        );

        let (present_complete_semaphore, rendering_complete_semaphore) =
            crate::vulkan::fence_semaphores::create_semaphores(&device).unwrap();

        let renderpass = crate::vulkan::renderpass::create_renderpass(&surface, &device).unwrap();

        let (descriptor_pool, descriptor_sets, desc_set_layouts) =
            crate::vulkan::descriptor_set::create_descriptor_set(&device);

        let (
            graphic_pipeline,
            viewports,
            scissors,
            graphics_pipelines,
            pipeline_layout,
            vertex_shader_module,
            fragment_shader_module,
        ) = crate::vulkan::pipeline::create_pipeline(
            &device,
            &surface,
            renderpass,
            desc_set_layouts,
        );

        let framebuffers = crate::vulkan::framebuffer::create_framebuffers(
            &device,
            &surface,
            &present_image_views,
            depth_image_view,
            renderpass,
        )
        .unwrap();

        let pool =
            crate::vulkan::command_pools::create_command_pools(&device, surface.queue_family_index)
                .unwrap();

        let (setup_command_buffer, draw_command_buffer) =
            crate::vulkan::command_buffers::create_command_buffers(&device, pool).unwrap();

        crate::vulkan::record::record_submit_commandbuffer(
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

                unsafe {
                    device.cmd_pipeline_barrier(
                        setup_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[layout_transition_barriers],
                    )
                };
            },
        );

        // MARK: UNIFORM BUFFER
        let mut uniform = Mat4::IDENTITY;
        // TEMP: rotate UBO transfrom by 25% of PI
        uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, std::f32::consts::PI / 4.0);

        let (uniform_color_buffer, uniform_color_buffer_memory) =
            crate::vulkan::uniform::create_uniform_buffer(
                &device,
                &device_memory_properties,
                uniform,
            );

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
        let image_buffer = unsafe { device.create_buffer(&image_buffer_info, None).unwrap() };
        let image_buffer_memory_req =
            unsafe { device.get_buffer_memory_requirements(image_buffer) };
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
        let image_buffer_memory = unsafe {
            device
                .allocate_memory(&image_buffer_allocate_info, None)
                .unwrap()
        };
        let image_ptr = unsafe {
            device
                .map_memory(
                    image_buffer_memory,
                    0,
                    image_buffer_memory_req.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap()
        };
        let mut image_slice = unsafe {
            Align::new(
                image_ptr,
                mem::align_of::<u8>() as u64,
                image_buffer_memory_req.size,
            )
        };
        image_slice.copy_from_slice(&image_data);
        unsafe {
            device.unmap_memory(image_buffer_memory);
            device
                .bind_buffer_memory(image_buffer, image_buffer_memory, 0)
                .unwrap();
        }

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
        let texture_image = unsafe { device.create_image(&texture_create_info, None).unwrap() };
        let texture_memory_req = unsafe { device.get_image_memory_requirements(texture_image) };
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
        let texture_memory = unsafe {
            device
                .allocate_memory(&texture_allocate_info, None)
                .unwrap()
        };
        unsafe {
            device
                .bind_image_memory(texture_image, texture_memory, 0)
                .expect("Unable to bind depth image memory")
        };

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
                unsafe {
                    device.cmd_pipeline_barrier(
                        texture_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[texture_barrier],
                    )
                };
                let buffer_copy_regions = vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(image_extent.into());

                unsafe {
                    device.cmd_copy_buffer_to_image(
                        texture_command_buffer,
                        image_buffer,
                        texture_image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[buffer_copy_regions],
                    )
                };
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
                unsafe {
                    device.cmd_pipeline_barrier(
                        texture_command_buffer,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[texture_barrier_end],
                    )
                };
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

        let texture_sampler = unsafe { device.create_sampler(&sampler_info, None).unwrap() };

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
        let tex_image_view = unsafe {
            device
                .create_image_view(&tex_image_view_info, None)
                .unwrap()
        };

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
        unsafe { device.update_descriptor_sets(&write_desc_sets, &[]) };

        // MARK: MESHES
        // TODO seeded RNG only
        let mut rng = rand::thread_rng();

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
                    offset,
                    offset + 1,
                    offset + 2,
                    offset,
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

        let swapchain_resources = AAAResources {
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

        drop(surface);

        let instance_arc = Arc::clone(&instance);
        let event_states_arc = Arc::clone(&self.event_states);
        let surface_loader_arc = Arc::clone(&surface_loader);
        let surface_arc = Arc::clone(&self.surface);
        self.render_handle = Some(thread::spawn(move || {
            let mut surface = surface_arc.lock().unwrap();
            surface.rendering_loop(
                instance_arc,
                event_states_arc,
                surface_loader_arc,
                uniform,
                image_buffer_memory,
                image_buffer,
                texture_memory,
                tex_image_view,
                texture_image,
                &desc_set_layouts,
                descriptor_pool,
                texture_sampler,
                uniform_color_buffer_memory,
                uniform_color_buffer,
                &graphics_pipelines,
                pool,
                swapchain,
                swapchain_resources,
                &mut device,
                &swapchain_loader,
                renderpass,
                &mut framebuffers.to_vec(),
                &mut viewports.to_vec(),
                &mut scissors.to_vec(),
                &descriptor_sets,
                pipeline_layout,
                graphic_pipeline,
                &registered_meshes,
                vertex_shader_module,
                fragment_shader_module,
                &mut device_memory_properties,
            )
        }));
    }

    pub fn destroy(&mut self, surface_loader: Arc<surface::Instance>) {
        self.event_states.close_requested();

        if let Some(handle) = self.render_handle.take() {
            handle.join().unwrap();
        }

        let surface_lock = self.surface.lock().unwrap();
        unsafe { surface_loader.destroy_surface(surface_lock.surface_khr, None) };
    }
}
