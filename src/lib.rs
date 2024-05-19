pub mod app;
mod debugging;
#[allow(dead_code, unused_variables)] // TEMP
mod input_manager;
mod metrics;
mod model;
mod shaders;
mod vulkan;

use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    util::Align,
    vk, Device, Entry, Instance,
};
use glam::Mat4;
use metrics::Metrics;
use model::{RegisteredMesh, Vertex};
use shaders::Shader;
use std::{
    default::Default, error::Error, ffi, mem, ops::Drop, os::raw::c_char, sync::mpsc,
    thread::JoinHandle,
};
use vulkan::{record::record_submit_commandbuffer, views::find_memorytype_index};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    Resize { width: u32, height: u32 },
}

struct EngineSurface {
    surface_khr: vk::SurfaceKHR,
    format: vk::SurfaceFormatKHR,
    capabilities: vk::SurfaceCapabilitiesKHR,
    resolution: vk::Extent2D,
}

struct EngineSwapchain {
    swapchain_khr: vk::SwapchainKHR,
    _desired_image_count: u32,
    _present_mode: vk::PresentModeKHR,
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

pub struct Renderer {
    _entry: Entry,

    instance: Instance,
    _pdevices: Vec<vk::PhysicalDevice>,
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

    descriptor_sets: Vec<vk::DescriptorSet>,
    desc_set_layouts: [vk::DescriptorSetLayout; 1],
    image_buffer_memory: vk::DeviceMemory,

    renderpass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    graphic_pipeline: vk::Pipeline,
    viewports: [vk::Viewport; 1],
    scissors: [vk::Rect2D; 1],
    graphics_pipelines: Vec<vk::Pipeline>,
    pipeline_layout: vk::PipelineLayout,
    vertex_shader_module: vk::ShaderModule,
    fragment_shader_module: vk::ShaderModule,

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

    receiver: mpsc::Receiver<UserEvent>,
}

impl Renderer {
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
            device_memory_properties,
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

    // #[profiling::function]
    // pub fn render(&mut self) {
    //     self.process_all_events();

    //     self.metrics.start_frame();
    //     let delta = self.metrics.delta_start_to_start;

    //     if self.minimized {
    //         return;
    //     }

    //     self.uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, delta.as_secs_f32());

    //     unsafe {
    //         Renderer::update_uniform_buffer(&self.device, self.uniform_buffer_memory, self.uniform);

    //         let result = self.swapchain_loader.acquire_next_image(
    //             self.swapchain.swapchain_khr,
    //             u64::MAX,
    //             self.swapchain_resources.present_complete_semaphore,
    //             vk::Fence::null(),
    //         );
    //         let (present_index, _) = match result {
    //             Ok(result) => result,
    //             Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return self.outdated_swapchain(),
    //             Err(err) => panic!("Failed to acquire next image: {:?}", err),
    //         };
    //         let clear_values = [
    //             vk::ClearValue {
    //                 color: vk::ClearColorValue {
    //                     float32: [0.0, 0.0, 0.0, 0.0],
    //                 },
    //             },
    //             vk::ClearValue {
    //                 depth_stencil: vk::ClearDepthStencilValue {
    //                     depth: 1.0,
    //                     stencil: 0,
    //                 },
    //             },
    //         ];

    //         let render_pass_begin_info = vk::RenderPassBeginInfo::default()
    //             .render_pass(self.renderpass)
    //             .framebuffer(self.framebuffers[present_index as usize])
    //             .render_area(self.surface.resolution.into())
    //             .clear_values(&clear_values);

    //         record_submit_commandbuffer(
    //             &self.device,
    //             self.swapchain_resources.draw_command_buffer,
    //             self.swapchain_resources.draw_commands_reuse_fence,
    //             self.swapchain.present_queue,
    //             &[vk::PipelineStageFlags::BOTTOM_OF_PIPE],
    //             &[self.swapchain_resources.present_complete_semaphore],
    //             &[self.swapchain_resources.rendering_complete_semaphore],
    //             |device, draw_command_buffer| {
    //                 device.cmd_begin_render_pass(
    //                     draw_command_buffer,
    //                     &render_pass_begin_info,
    //                     vk::SubpassContents::INLINE,
    //                 );
    //                 device.cmd_bind_descriptor_sets(
    //                     draw_command_buffer,
    //                     vk::PipelineBindPoint::GRAPHICS,
    //                     self.pipeline_layout,
    //                     0,
    //                     &self.descriptor_sets,
    //                     &[],
    //                 );
    //                 device.cmd_bind_pipeline(
    //                     draw_command_buffer,
    //                     vk::PipelineBindPoint::GRAPHICS,
    //                     self.graphic_pipeline,
    //                 );
    //                 device.cmd_set_viewport(draw_command_buffer, 0, &self.viewports);
    //                 device.cmd_set_scissor(draw_command_buffer, 0, &self.scissors);

    //                 for registered_mesh in &self.registered_meshes {
    //                     device.cmd_bind_vertex_buffers(
    //                         draw_command_buffer,
    //                         0,
    //                         &[registered_mesh.vertex_buffer],
    //                         &[0],
    //                     );
    //                     device.cmd_bind_index_buffer(
    //                         draw_command_buffer,
    //                         registered_mesh.index_buffer,
    //                         0,
    //                         vk::IndexType::UINT32,
    //                     );
    //                     device.cmd_draw_indexed(
    //                         draw_command_buffer,
    //                         registered_mesh.mesh.indices.len() as u32,
    //                         1,
    //                         0,
    //                         0,
    //                         1,
    //                     );
    //                 }

    //                 // Or draw without the index buffer
    //                 // device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
    //                 device.cmd_end_render_pass(draw_command_buffer);
    //             },
    //         );
    //         let wait_semaphors = [self.swapchain_resources.rendering_complete_semaphore];
    //         let swapchains = [self.swapchain.swapchain_khr];
    //         let image_indices = [present_index];
    //         let present_info = vk::PresentInfoKHR::default()
    //             .wait_semaphores(&wait_semaphors)
    //             .swapchains(&swapchains)
    //             .image_indices(&image_indices);
    //         let queue_present_result = self
    //             .swapchain_loader
    //             .queue_present(self.swapchain.present_queue, &present_info);

    //         match queue_present_result {
    //             Ok(_) => {}
    //             Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.outdated_swapchain(),
    //             Err(err) => panic!("Failed to present queue: {:?}", err),
    //         }
    //     }

    //     self.metrics.end_frame();
    // }

    pub fn start_update_thread(&self) -> JoinHandle<()> {
        std::thread::spawn(move || {
            // IMPLEMENTATION
        })
    }

    // fn outdated_swapchain(&mut self) {
    //     self.process_all_events();
    // }

    // pub fn process_all_events(&mut self) {
    //     loop {
    //         match self.receiver.try_recv() {
    //             Ok(event) => match event {
    //                 UserEvent::Resize { width, height } => {
    //                     println!("Processed resize event: {}x{}", width, height);
    //                     self.recreate_swapchain(width, height);
    //                 }
    //             },
    //             _ => break,
    //         }
    //     }
    // }

    // pub fn recreate_swapchain(&mut self, width: u32, height: u32) {
    //     if width == 0 || height == 0 {
    //         return self.minimized = true;
    //     }

    //     self.minimized = false;

    //     if width == self.surface.resolution.width && height == self.surface.resolution.height {
    //         return;
    //     }

    //     unsafe {
    //         self.device.device_wait_idle().unwrap();

    //         self.destroy_swapchain();

    //         // surface
    //         self.surface.resolution = vk::Extent2D { width, height };
    //         self.surface.capabilities = self
    //             .surface_loader
    //             .get_physical_device_surface_capabilities(self.pdevice, self.surface.surface_khr)
    //             .unwrap();

    //         // viewports and scissors
    //         self.viewports = [vk::Viewport {
    //             x: 0.0,
    //             y: 0.0,
    //             width: width as f32,
    //             height: height as f32,
    //             min_depth: 0.0,
    //             max_depth: 1.0,
    //         }];
    //         self.scissors = [vk::Rect2D {
    //             offset: vk::Offset2D { x: 0, y: 0 },
    //             extent: vk::Extent2D { width, height },
    //         }];

    //         // swapchain
    //         self.swapchain = Self::create_swapchain(
    //             &self.device,
    //             &self.surface_loader,
    //             &self.surface,
    //             self.pdevice,
    //             self.queue_family_index,
    //             width,
    //             height,
    //             &self.swapchain_loader,
    //         )
    //         .unwrap();

    //         // image_views
    //         let (
    //             present_images,
    //             present_image_views,
    //             depth_image_view,
    //             depth_image,
    //             depth_image_memory,
    //             device_memory_properties,
    //         ) = Self::create_views_and_depth(
    //             &self.device,
    //             &self.instance,
    //             &self.swapchain,
    //             &self.surface,
    //             &self.pdevice,
    //             &self.swapchain_loader,
    //         );

    //         self.swapchain_resources.present_images = present_images;
    //         self.swapchain_resources.present_image_views = present_image_views;
    //         self.swapchain_resources.depth_image_view = depth_image_view;
    //         self.swapchain_resources.depth_image = depth_image;
    //         self.swapchain_resources.depth_image_memory = depth_image_memory;
    //         self.device_memory_properties = device_memory_properties;

    //         // framebuffers
    //         let framebuffers = Self::create_framebuffers(
    //             &self.device,
    //             &self.surface,
    //             &self.swapchain_resources.present_image_views,
    //             depth_image_view,
    //             self.renderpass,
    //         )
    //         .unwrap();

    //         self.framebuffers = framebuffers;

    //         // register depth image memory
    //         record_submit_commandbuffer(
    //             &self.device,
    //             self.swapchain_resources.setup_command_buffer,
    //             self.swapchain_resources.setup_commands_reuse_fence,
    //             self.swapchain.present_queue,
    //             &[],
    //             &[],
    //             &[],
    //             |device, setup_command_buffer| {
    //                 let layout_transition_barriers = vk::ImageMemoryBarrier::default()
    //                     .image(depth_image)
    //                     .dst_access_mask(
    //                         vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
    //                             | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
    //                     )
    //                     .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
    //                     .old_layout(vk::ImageLayout::UNDEFINED)
    //                     .subresource_range(
    //                         vk::ImageSubresourceRange::default()
    //                             .aspect_mask(vk::ImageAspectFlags::DEPTH)
    //                             .layer_count(1)
    //                             .level_count(1),
    //                     );

    //                 device.cmd_pipeline_barrier(
    //                     setup_command_buffer,
    //                     vk::PipelineStageFlags::BOTTOM_OF_PIPE,
    //                     vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
    //                     vk::DependencyFlags::empty(),
    //                     &[],
    //                     &[],
    //                     &[layout_transition_barriers],
    //                 );
    //             },
    //         );

    //         self.render();
    //     }
    // }

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

impl Drop for EngineSwapchain {
    fn drop(&mut self) {
        // unsafe {
        //     self.device.device_wait_idle().unwrap();

        //     self.device
        //         .destroy_semaphore(self.present_complete_semaphore, None);
        //     self.device
        //         .destroy_semaphore(self.rendering_complete_semaphore, None);
        //     self.device
        //         .destroy_fence(self.draw_commands_reuse_fence, None);
        //     self.device
        //         .destroy_fence(self.setup_commands_reuse_fence, None);
        //     self.device.destroy_command_pool(self.pool, None);
        // }
    }
}

impl Drop for Renderer {
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

            self.surface_loader
                .destroy_surface(self.surface.surface_khr, None);

            self.debug_utils_loader
                .destroy_debug_utils_messenger(self.debug_call_back, None); // NOTE PORTED

            self.instance.destroy_instance(None); // NOTE PORTED
        }
    }
}
