use crate::input_manager::InputState;
use crate::model::{Mesh, Vertex};
use crate::shaders::Shader;
use crate::vulkan::debug_callback::DebugUtils;
use crate::vulkan::record::record_submit_commandbuffer;
use crate::vulkan::surface::{AAAResources, AAASurface};
use crate::vulkan::Destroy;
use crate::{find_memorytype_index, UserEvent};
use ash::khr::{surface, swapchain};
use ash::util::Align;
use ash::vk::{self, PhysicalDevice};
use ash::Entry;
use cursor_icon::CursorIcon;
use glam::Mat4;
use log::info;
use rand::Rng;
use rwh_06::HasDisplayHandle;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::{fmt, mem, thread};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState};
use winit::window::{
    Cursor, CursorGrabMode, CustomCursor, CustomCursorSource, Fullscreen, Icon, ResizeDirection,
    Theme, Window, WindowId,
};

#[cfg(macos_platform)]
use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS, WindowExtMacOS};
#[cfg(any(x11_platform, wayland_platform))]
use winit::platform::startup_notify::{
    self, EventLoopExtStartupNotify, WindowAttributesExtStartupNotify, WindowExtStartupNotify,
};

const WIN_TITLE: &str = "Pulsar";
/// The amount of points to around the window for drag resize direction calculations.
const BORDER_SIZE: f64 = 20.;
pub const WIN_START_INNER_SIZE: PhysicalSize<u32> = PhysicalSize::new(1280, 720);

/// Application state and event handling.
pub struct Application {
    /// Custom cursors assets.
    custom_cursors: Vec<CustomCursor>,
    /// Application icon.
    icon: Icon,
    windows: HashMap<WindowId, WindowState>,

    entry: Entry,
    instance: ash::Instance,
    surface_loader: surface::Instance,
    #[cfg(debug_assertions)]
    debug_utils: DebugUtils,

    physical_device_list: Vec<PhysicalDevice>,

    input_state: Arc<InputState>,
}

impl Application {
    pub fn new<T>(event_loop: &EventLoop<T>) -> Result<Self, Box<dyn Error>> {
        env_logger::init();

        #[cfg(debug_assertions)]
        Shader::compile_shaders();

        // You'll have to choose an icon size at your own discretion. On X11, the desired size
        // varies by WM, and on Windows, you still have to account for screen scaling. Here
        // we use 32px, since it seems to work well enough in most cases. Be careful about
        // going too high, or you'll be bitten by the low-quality downscaling built into the
        // WM.
        let icon = load_icon(include_bytes!("../assets/img/icon.png"));

        // info!("Loading cursor assets");
        let custom_cursors = vec![
            event_loop
                .create_custom_cursor(decode_cursor(include_bytes!("../assets/img/cross.png"))),
            event_loop
                .create_custom_cursor(decode_cursor(include_bytes!("../assets/img/cross2.png"))),
            event_loop
                .create_custom_cursor(decode_cursor(include_bytes!("../assets/img/gradient.png"))),
        ];

        let entry = Entry::linked();

        let instance =
            crate::vulkan::instance::create_instance(&entry, event_loop.display_handle().unwrap())?;

        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

        #[cfg(debug_assertions)]
        let debug_utils = DebugUtils::new(&entry, &instance)?;

        let physical_device_list = unsafe {
            instance
                .enumerate_physical_devices()
                .expect("Physical device error")
        };

        // create artist and send it in a new thread
        // let artist = Artist::new();
        // let artist_arc_rwlock = Arc::new(RwLock::new(artist));

        // let artist_arc_rwlock_clone = artist_arc_rwlock.clone();
        // thread::spawn(move || {
        //     let artist = artist_arc_rwlock_clone.read().unwrap();
        //     artist.run();
        // });

        Ok(Self {
            custom_cursors,
            icon,
            windows: Default::default(),

            entry,
            instance,
            surface_loader,
            #[cfg(debug_assertions)]
            debug_utils,
            physical_device_list,

            input_state: Arc::new(Default::default()),
        })
    }

    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        _tab_id: Option<String>,
    ) -> Result<WindowId, Box<dyn Error>> {
        // TODO read-out activation token.

        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes()
            .with_title(WIN_TITLE)
            .with_transparent(true)
            .with_window_icon(Some(self.icon.clone()))
            .with_inner_size(WIN_START_INNER_SIZE);

        #[cfg(any(x11_platform, wayland_platform))]
        if let Some(token) = event_loop.read_token_from_env() {
            startup_notify::reset_activation_token_env();
            // info!("Using token {:?} to activate a window", token);
            window_attributes = window_attributes.with_activation_token(token);
        }

        #[cfg(macos_platform)]
        if let Some(tab_id) = _tab_id {
            window_attributes = window_attributes.with_tabbing_identifier(&tab_id);
        }

        let window = event_loop.create_window(window_attributes)?;

        #[cfg(ios_platform)]
        {
            use winit::platform::ios::WindowExtIOS;
            window.recognize_doubletap_gesture(true);
            window.recognize_pinch_gesture(true);
            window.recognize_rotation_gesture(true);
            window.recognize_pan_gesture(true, 2, 2);
        }

        let (sender, receiver) = mpsc::channel::<UserEvent>();

        let window_state = WindowState::new(self, window, sender, Some(receiver))?;
        let window_id = window_state.window.id();
        // info!("Created new window with id={window_id:?}");
        self.windows.insert(window_id, window_state);
        Ok(window_id)
    }

    fn handle_action(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, action: Action) {
        // let cursor_position = self.cursor_position;
        let window = self.windows.get_mut(&window_id).unwrap();
        // info!("Executing action: {action:?}");
        match action {
            Action::CloseWindow => {
                self.windows.remove(&window_id).unwrap();
            }
            Action::CreateNewWindow => {
                #[cfg(any(x11_platform, wayland_platform))]
                if let Err(err) = window.window.request_activation_token() {
                    info!("Failed to get activation token: {err}");
                } else {
                    return;
                }

                if let Err(err) = self.create_window(event_loop, None) {
                    panic!("Error creating new window: {err}");
                }
            }
            Action::ToggleResizeIncrements => window.toggle_resize_increments(),
            Action::ToggleCursorVisibility => window.toggle_cursor_visibility(),
            Action::ToggleResizable => window.toggle_resizable(),
            Action::ToggleDecorations => window.toggle_decorations(),
            Action::ToggleFullscreen => window.toggle_fullscreen(),
            Action::ToggleMaximize => window.toggle_maximize(),
            Action::ToggleImeInput => window.toggle_ime(),
            Action::Minimize => window.minimize(),
            Action::NextCursor => window.next_cursor(),
            Action::NextCustomCursor => window.next_custom_cursor(&self.custom_cursors),
            Action::CycleCursorGrab => window.cycle_cursor_grab(),
            Action::DragWindow => window.drag_window(),
            Action::DragResizeWindow => window.drag_resize_window(),
            Action::ShowWindowMenu => window.show_menu(),
            Action::PrintHelp => self.print_help(),
            #[cfg(macos_platform)]
            Action::CycleOptionAsAlt => window.cycle_option_as_alt(),
            #[cfg(macos_platform)]
            Action::CreateNewTab => {
                let tab_id = window.window.tabbing_identifier();
                if let Err(err) = self.create_window(event_loop, Some(tab_id)) {
                    error!("Error creating new window: {err}");
                }
            }
            Action::RequestResize => window.swap_dimensions(),
        }
    }

    fn dump_monitors(&self, event_loop: &ActiveEventLoop) {
        // info!("Monitors information");
        let primary_monitor = event_loop.primary_monitor();
        for monitor in event_loop.available_monitors() {
            let intro = if primary_monitor.as_ref() == Some(&monitor) {
                "Primary monitor"
            } else {
                "Monitor"
            };

            if let Some(name) = monitor.name() {
                info!("{intro}: {name}");
            } else {
                info!("{intro}: [no name]");
            }

            let PhysicalSize { width, height } = monitor.size();
            info!(
                "  Current mode: {width}x{height}{}",
                if let Some(m_hz) = monitor.refresh_rate_millihertz() {
                    format!(" @ {}.{} Hz", m_hz / 1000, m_hz % 1000)
                } else {
                    String::new()
                }
            );

            let PhysicalPosition { x, y } = monitor.position();
            info!("  Position: {x},{y}");

            info!("  Scale factor: {}", monitor.scale_factor());

            info!("  Available modes (width x height x bit-depth):");
            for mode in monitor.video_modes() {
                let PhysicalSize { width, height } = mode.size();
                let bits = mode.bit_depth();
                let m_hz = mode.refresh_rate_millihertz();
                info!(
                    "    {width}x{height}x{bits} @ {}.{} Hz",
                    m_hz / 1000,
                    m_hz % 1000
                );
            }
        }
    }

    /// Process the key binding.
    fn process_key_binding(key: &str, mods: &ModifiersState) -> Option<Action> {
        KEY_BINDINGS.iter().find_map(|binding| {
            binding
                .is_triggered_by(&key, mods)
                .then_some(binding.action)
        })
    }

    /// Process mouse binding.
    fn process_mouse_binding(button: MouseButton, mods: &ModifiersState) -> Option<Action> {
        MOUSE_BINDINGS.iter().find_map(|binding| {
            binding
                .is_triggered_by(&button, mods)
                .then_some(binding.action)
        })
    }

    fn print_help(&self) {
        info!("Keyboard bindings:");
        for binding in KEY_BINDINGS {
            info!(
                "{}{:<10} - {} ({})",
                modifiers_to_string(binding.mods),
                binding.trigger,
                binding.action,
                binding.action.help(),
            );
        }
        info!("Mouse bindings:");
        for binding in MOUSE_BINDINGS {
            info!(
                "{}{:<10} - {} ({})",
                modifiers_to_string(binding.mods),
                mouse_button_to_string(binding.trigger),
                binding.action,
                binding.action.help(),
            );
        }
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        self.debug_utils.destroy();

        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

impl ApplicationHandler<UserEvent> for Application {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        info!("User event: {event:?}");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let window = match self.windows.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        match event {
            WindowEvent::Resized(size) => {
                window.resize(size);
            }
            WindowEvent::Focused(focused) => {
                if focused {
                    info!("Window={window_id:?} focused");
                } else {
                    info!("Window={window_id:?} unfocused");
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                info!("Window={window_id:?} changed scale to {scale_factor}");
            }
            WindowEvent::ThemeChanged(theme) => {
                info!("Theme changed to {theme:?}");
                window.set_theme(theme);
            }
            WindowEvent::RedrawRequested => {}
            WindowEvent::Occluded(occluded) => {
                window.set_occluded(occluded);
            }
            WindowEvent::CloseRequested => {
                info!("Closing Window={window_id:?}");
                let window_state = self.windows.remove(&window_id).unwrap();
                window_state.rendering.store(false, Ordering::Relaxed);
                window_state.render_handle.unwrap().join().unwrap();
                let surface_arc = window_state.surface;
                let surface_lock = surface_arc.lock().unwrap();
                unsafe {
                    self.surface_loader
                        .destroy_surface(surface_lock.surface_khr, None)
                };
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                window.modifiers = modifiers.state();
                info!("Modifiers changed to {:?}", window.modifiers);
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    info!("Mouse wheel Line Delta: ({x},{y})");
                }
                MouseScrollDelta::PixelDelta(px) => {
                    info!("Mouse wheel Pixel Delta: ({},{})", px.x, px.y);
                }
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                let mods = window.modifiers;

                // Dispatch actions only on press.
                if event.state.is_pressed() {
                    let action = if let Key::Character(ch) = event.logical_key.as_ref() {
                        Self::process_key_binding(&ch.to_uppercase(), &mods)
                    } else {
                        None
                    };

                    if let Some(action) = action {
                        self.handle_action(event_loop, window_id, action);
                    }
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                let mods = window.modifiers;
                if let Some(action) = state
                    .is_pressed()
                    .then(|| Self::process_mouse_binding(button, &mods))
                    .flatten()
                {
                    self.handle_action(event_loop, window_id, action);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                // info!("Cursor left Window={window_id:?}");
                window.cursor_left();
            }
            WindowEvent::CursorMoved { position, .. } => {
                // info!("Moved cursor to {position:?}");
                window.cursor_moved(position);
            }
            WindowEvent::ActivationTokenDone { token: _token, .. } => {
                #[cfg(any(x11_platform, wayland_platform))]
                {
                    startup_notify::set_activation_token_env(_token);
                    if let Err(err) = self.create_window(event_loop, None) {
                        error!("Error creating new window: {err}");
                    }
                }
            }
            WindowEvent::Ime(event) => match event {
                Ime::Enabled => {} // info!("IME enabled for Window={window_id:?}"),
                Ime::Preedit(text, caret_pos) => {
                    info!("Preedit: {}, with caret at {:?}", text, caret_pos);
                }
                Ime::Commit(text) => {
                    info!("Committed: {}", text);
                }
                Ime::Disabled => info!("IME disabled for Window={window_id:?}"),
            },
            WindowEvent::PinchGesture { delta, .. } => {
                window.zoom += delta;
                let zoom = window.zoom;
                if delta > 0.0 {
                    info!("Zoomed in {delta:.5} (now: {zoom:.5})");
                } else {
                    info!("Zoomed out {delta:.5} (now: {zoom:.5})");
                }
            }
            WindowEvent::RotationGesture { delta, .. } => {
                window.rotated += delta;
                let rotated = window.rotated;
                if delta > 0.0 {
                    info!("Rotated counterclockwise {delta:.5} (now: {rotated:.5})");
                } else {
                    info!("Rotated clockwise {delta:.5} (now: {rotated:.5})");
                }
            }
            WindowEvent::PanGesture { delta, phase, .. } => {
                window.panned.x += delta.x;
                window.panned.y += delta.y;
                info!("Panned ({delta:?})) (now: {:?}), {phase:?}", window.panned);
            }
            WindowEvent::DoubleTapGesture { .. } => {
                info!("Smart zoom");
            }
            WindowEvent::TouchpadPressure { .. }
            | WindowEvent::HoveredFileCancelled
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::AxisMotion { .. }
            | WindowEvent::DroppedFile(_)
            | WindowEvent::HoveredFile(_)
            | WindowEvent::Destroyed
            | WindowEvent::Touch(_)
            | WindowEvent::Moved(_) => (),
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        _event: DeviceEvent,
    ) {
        // info!("Device {device_id:?} event: {event:?}");
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Resumed the event loop");
        self.dump_monitors(event_loop);

        // Create initial window.
        let window_id = self
            .create_window(event_loop, None)
            .expect("failed to create initial window");
        let window_state = self.windows.get_mut(&window_id).unwrap();

        let surface = window_state.surface.lock().unwrap();

        let mut device = crate::vulkan::device::create_device(
            &self.instance,
            surface.physical_device,
            surface.queue_family_index,
        )
        .unwrap();

        let swapchain_loader = swapchain::Device::new(&self.instance, &device);

        // TODO get from os window api for linux and possibly more
        let size = surface.capabilities.current_extent;

        let swapchain = crate::vulkan::swapchain::AAASwapchain::new(
            &device,
            &self.surface_loader,
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
            device_memory_properties,
        ) = crate::vulkan::views::create_views_and_depth(
            &device,
            &self.instance,
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

        let rendering_clone = window_state.rendering.clone();

        drop(surface);
        let surface_clone = window_state.surface.clone();
        window_state.render_handle = Some(thread::spawn(move || {
            let surface = surface_clone.lock().unwrap();
            surface.render(
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
                rendering_clone,
                &swapchain,
                &swapchain_resources,
                &mut device,
                &swapchain_loader,
                renderpass,
                &framebuffers,
                &viewports,
                &scissors,
                &descriptor_sets,
                pipeline_layout,
                graphic_pipeline,
                &registered_meshes,
                vertex_shader_module,
                fragment_shader_module,
            )
        }));

        self.print_help();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows.is_empty() {
            // info!("No windows left, exiting...");
            event_loop.exit();
        }
    }

    #[cfg(not(any(android_platform, ios_platform)))]
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        //
    }
}

/// State of the window.
struct WindowState {
    /// IME input.
    ime: bool,
    /// Render surface.
    renderer_sender: mpsc::Sender<UserEvent>,
    renderer_receiver: Option<mpsc::Receiver<UserEvent>>,
    /// The actual winit Window.
    window: Arc<Window>,
    /// The window theme we're drawing with.
    theme: Theme,
    /// Cursor position over the window.
    cursor_position: Option<PhysicalPosition<f64>>,
    /// Window modifiers state.
    modifiers: ModifiersState,
    /// Occlusion state of the window.
    occluded: bool,
    /// Current cursor grab mode.
    cursor_grab: CursorGrabMode,
    /// The amount of zoom into window.
    zoom: f64,
    /// The amount of rotation of the window.
    rotated: f32,
    /// The amount of pan of the window.
    panned: PhysicalPosition<f32>,

    #[cfg(macos_platform)]
    option_as_alt: OptionAsAlt,

    // Cursor states.
    named_idx: usize,
    custom_idx: usize,
    cursor_hidden: bool,

    // Render
    surface: Arc<Mutex<AAASurface>>,
    rendering: Arc<AtomicBool>,
    render_handle: Option<thread::JoinHandle<()>>,
}

impl WindowState {
    fn new(
        app: &Application,
        window: Window,
        renderer_sender: mpsc::Sender<UserEvent>,
        renderer_receiver: Option<mpsc::Receiver<UserEvent>>,
    ) -> Result<Self, Box<dyn Error>> {
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
            renderer_sender,
            renderer_receiver,
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
            rendering: Arc::new(AtomicBool::new(true)),
            surface: Arc::new(Mutex::new(surface)),
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
    fn toggle_maximize(&self) {
        let maximized = self.window.is_maximized();
        self.window.set_maximized(!maximized);
    }

    /// Toggle window decorations.
    fn toggle_decorations(&self) {
        let decorated = self.window.is_decorated();
        self.window.set_decorations(!decorated);
    }

    /// Toggle window resizable state.
    fn toggle_resizable(&self) {
        let resizable = self.window.is_resizable();
        self.window.set_resizable(!resizable);
    }

    /// Toggle cursor visibility
    fn toggle_cursor_visibility(&mut self) {
        self.cursor_hidden = !self.cursor_hidden;
        self.window.set_cursor_visible(!self.cursor_hidden);
    }

    /// Toggle resize increments on a window.
    fn toggle_resize_increments(&mut self) {
        let new_increments = match self.window.resize_increments() {
            Some(_) => None,
            None => Some(LogicalSize::new(25.0, 25.0)),
        };
        // info!("Had increments: {}", new_increments.is_none());
        self.window.set_resize_increments(new_increments);
    }

    /// Toggle fullscreen.
    fn toggle_fullscreen(&self) {
        let fullscreen = if self.window.fullscreen().is_some() {
            None
        } else {
            Some(Fullscreen::Borderless(None))
        };

        self.window.set_fullscreen(fullscreen);
    }

    /// Cycle through the grab modes ignoring errors.
    fn cycle_cursor_grab(&mut self) {
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
    fn swap_dimensions(&mut self) {
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

    /// Pick the next cursor.
    fn next_cursor(&mut self) {
        self.named_idx = (self.named_idx + 1) % CURSORS.len();
        // info!("Setting cursor to \"{:?}\"", CURSORS[self.named_idx]);
        self.window
            .set_cursor(Cursor::Icon(CURSORS[self.named_idx]));
    }

    /// Pick the next custom cursor.
    fn next_custom_cursor(&mut self, custom_cursors: &[CustomCursor]) {
        self.custom_idx = (self.custom_idx + 1) % custom_cursors.len();
        let cursor = Cursor::Custom(custom_cursors[self.custom_idx].clone());
        self.window.set_cursor(cursor);
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        // info!("Resized to {size:?}");
        #[cfg(not(any(android_platform, ios_platform)))]
        {
            self.renderer_sender
                .send(UserEvent::Resize {
                    width: size.width,
                    height: size.height,
                })
                .expect("failed to send resize event");

            // input_manager
            // let mut input_manager = self.input_manager_arc_rwlock.write().unwrap();
            // input_manager.window_size = size;
        }
    }

    /// Change the theme.
    fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Show window menu.
    fn show_menu(&self) {
        if let Some(position) = self.cursor_position {
            self.window.show_window_menu(position);
        }
    }

    /// Drag the window.
    fn drag_window(&self) {
        if let Err(err) = self.window.drag_window() {
            info!("Error starting window drag: {err}");
        } else {
            info!("Dragging window Window={:?}", self.window.id());
        }
    }

    /// Drag-resize the window.
    fn drag_resize_window(&self) {
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
    fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
        if occluded {
            // TODO stop rendering
        }
    }
}

struct Binding<T: Eq> {
    trigger: T,
    mods: ModifiersState,
    action: Action,
}

impl<T: Eq> Binding<T> {
    const fn new(trigger: T, mods: ModifiersState, action: Action) -> Self {
        Self {
            trigger,
            mods,
            action,
        }
    }

    fn is_triggered_by(&self, trigger: &T, mods: &ModifiersState) -> bool {
        &self.trigger == trigger && &self.mods == mods
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    CloseWindow,
    ToggleCursorVisibility,
    CreateNewWindow,
    ToggleResizeIncrements,
    ToggleImeInput,
    ToggleDecorations,
    ToggleResizable,
    ToggleFullscreen,
    ToggleMaximize,
    Minimize,
    NextCursor,
    NextCustomCursor,
    CycleCursorGrab,
    PrintHelp,
    DragWindow,
    DragResizeWindow,
    ShowWindowMenu,
    #[cfg(macos_platform)]
    CycleOptionAsAlt,
    #[cfg(macos_platform)]
    CreateNewTab,
    RequestResize,
}

impl Action {
    fn help(&self) -> &'static str {
        match self {
            Action::CloseWindow => "Close window",
            Action::ToggleCursorVisibility => "Hide cursor",
            Action::CreateNewWindow => "Create new window",
            Action::ToggleImeInput => "Toggle IME input",
            Action::ToggleDecorations => "Toggle decorations",
            Action::ToggleResizable => "Toggle window resizable state",
            Action::ToggleFullscreen => "Toggle fullscreen",
            Action::ToggleMaximize => "Maximize",
            Action::Minimize => "Minimize",
            Action::ToggleResizeIncrements => "Use resize increments when resizing window",
            Action::NextCursor => "Advance the cursor to the next value",
            Action::NextCustomCursor => "Advance custom cursor to the next value",
            Action::CycleCursorGrab => "Cycle through cursor grab mode",
            Action::PrintHelp => "Print help",
            Action::DragWindow => "Start window drag",
            Action::DragResizeWindow => "Start window drag-resize",
            Action::ShowWindowMenu => "Show window menu",
            #[cfg(macos_platform)]
            Action::CycleOptionAsAlt => "Cycle option as alt mode",
            #[cfg(macos_platform)]
            Action::CreateNewTab => "Create new tab",
            Action::RequestResize => "Request a resize",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self, f)
    }
}

fn decode_cursor(bytes: &[u8]) -> CustomCursorSource {
    let img = image::load_from_memory(bytes).unwrap().to_rgba8();
    let samples = img.into_flat_samples();
    let (_, w, h) = samples.extents();
    let (w, h) = (w as u16, h as u16);
    CustomCursor::from_rgba(samples.samples, w, h, w / 2, h / 2).unwrap()
}

fn load_icon(bytes: &[u8]) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(bytes).unwrap().into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}

fn modifiers_to_string(mods: ModifiersState) -> String {
    [
        (ModifiersState::SUPER, "Super+"),
        (ModifiersState::ALT, "Alt+"),
        (ModifiersState::CONTROL, "Ctrl+"),
        (ModifiersState::SHIFT, "Shift+"),
    ]
    .iter()
    .filter(|(modifier, _)| mods.contains(*modifier))
    .map(|(_, desc)| *desc)
    .collect::<String>()
}

fn mouse_button_to_string(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "LMB",
        MouseButton::Right => "RMB",
        MouseButton::Middle => "MMB",
        MouseButton::Back => "Back",
        MouseButton::Forward => "Forward",
        MouseButton::Other(_) => "Other",
    }
}

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

const KEY_BINDINGS: &[Binding<&'static str>] = &[
    Binding::new("Q", ModifiersState::CONTROL, Action::CloseWindow),
    Binding::new("H", ModifiersState::CONTROL, Action::PrintHelp),
    Binding::new("F", ModifiersState::CONTROL, Action::ToggleFullscreen),
    Binding::new("D", ModifiersState::CONTROL, Action::ToggleDecorations),
    Binding::new("I", ModifiersState::CONTROL, Action::ToggleImeInput),
    Binding::new("L", ModifiersState::CONTROL, Action::CycleCursorGrab),
    Binding::new("P", ModifiersState::CONTROL, Action::ToggleResizeIncrements),
    Binding::new("R", ModifiersState::CONTROL, Action::ToggleResizable),
    Binding::new("R", ModifiersState::ALT, Action::RequestResize),
    // M.
    Binding::new("M", ModifiersState::CONTROL, Action::ToggleMaximize),
    Binding::new("M", ModifiersState::ALT, Action::Minimize),
    // N.
    Binding::new("N", ModifiersState::CONTROL, Action::CreateNewWindow),
    // C.
    Binding::new("C", ModifiersState::CONTROL, Action::NextCursor),
    Binding::new("C", ModifiersState::ALT, Action::NextCustomCursor),
    Binding::new("Z", ModifiersState::CONTROL, Action::ToggleCursorVisibility),
    #[cfg(macos_platform)]
    Binding::new("T", ModifiersState::SUPER, Action::CreateNewTab),
    #[cfg(macos_platform)]
    Binding::new("O", ModifiersState::CONTROL, Action::CycleOptionAsAlt),
];

const MOUSE_BINDINGS: &[Binding<MouseButton>] = &[
    Binding::new(
        MouseButton::Left,
        ModifiersState::ALT,
        Action::DragResizeWindow,
    ),
    Binding::new(
        MouseButton::Left,
        ModifiersState::CONTROL,
        Action::DragWindow,
    ),
    Binding::new(
        MouseButton::Right,
        ModifiersState::CONTROL,
        Action::ShowWindowMenu,
    ),
];
