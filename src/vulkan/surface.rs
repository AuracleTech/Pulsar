use super::{swapchain::AAASwapchain, Destroy};
use crate::{input_manager::EventStates, metrics::Metrics, model::RegisteredMesh};
use ash::{
    khr::{surface, swapchain},
    util::Align,
    vk, Entry,
};
use glam::Mat4;
use log::debug;
use rwh_06::{HasDisplayHandle, HasWindowHandle};
use std::{
    error::Error,
    mem,
    sync::{atomic::Ordering, Arc},
};

pub struct AAASurface {
    pub surface_khr: vk::SurfaceKHR,
    pub format: vk::SurfaceFormatKHR,
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub resolution: vk::Extent2D,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_index: u32,
}

pub struct AAAResources {
    pub draw_command_buffer: vk::CommandBuffer,
    pub setup_command_buffer: vk::CommandBuffer,

    pub depth_image: vk::Image,
    pub depth_image_view: vk::ImageView,
    pub depth_image_memory: vk::DeviceMemory,

    pub present_images: Vec<vk::Image>,
    pub present_image_views: Vec<vk::ImageView>,

    pub draw_commands_reuse_fence: vk::Fence,
    pub setup_commands_reuse_fence: vk::Fence,

    pub present_complete_semaphore: vk::Semaphore,
    pub rendering_complete_semaphore: vk::Semaphore,
}

impl AAASurface {
    pub fn new(
        entry: &Entry,
        instance: &ash::Instance,
        window: &winit::window::Window,
        pdevices: &[ash::vk::PhysicalDevice],
        surface_loader: &surface::Instance,
    ) -> Result<Self, Box<dyn Error>> {
        let surface = unsafe {
            ash_window::create_surface(
                entry,
                instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )
            .unwrap()
        };

        let (pdevice, queue_family_index) = pdevices
            .iter()
            .find_map(|pdevice| unsafe {
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

        let surface_format = unsafe {
            surface_loader
                .get_physical_device_surface_formats(pdevice, surface)
                .unwrap()[0]
        };

        let surface_capabilities = unsafe {
            surface_loader
                .get_physical_device_surface_capabilities(pdevice, surface)
                .unwrap()
        };

        Ok(Self {
            surface_khr: surface,
            format: surface_format,
            capabilities: surface_capabilities,
            resolution: surface_capabilities.current_extent,
            physical_device: pdevice,
            queue_family_index,
        })
    }

    // pub fn update(&self, uniform: Mat4) {
    //     self.uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 5); // TODO reinplement delta time
    //     self.update_uniform_buffer(&self.device, self.uniform_buffer_memory, self.uniform);
    // }

    fn update_uniform_buffer(
        device: &ash::Device,
        uniform_buffer_memory: vk::DeviceMemory,
        new_transform: Mat4,
    ) {
        unsafe {
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
    }

    pub fn render(
        &self,
        event_states: Arc<EventStates>,

        // minimized: Arc<AtomicBool>,
        // stopping: Arc<AtomicBool>,
        mut uniform: Mat4,
        image_buffer_memory: vk::DeviceMemory,
        image_buffer: vk::Buffer,
        texture_memory: vk::DeviceMemory,
        tex_image_view: vk::ImageView,
        texture_image: vk::Image,
        desc_set_layouts: &[vk::DescriptorSetLayout],
        descriptor_pool: vk::DescriptorPool,
        texture_sampler: vk::Sampler,
        uniform_color_buffer_memory: vk::DeviceMemory,
        uniform_color_buffer: vk::Buffer,
        graphics_pipelines: &[vk::Pipeline],
        pool: vk::CommandPool,
        swapchain: &AAASwapchain,
        swapchain_resources: &AAAResources,
        device: &mut ash::Device,
        swapchain_loader: &swapchain::Device,
        renderpass: vk::RenderPass,
        framebuffers: &[vk::Framebuffer],
        viewports: &[vk::Viewport],
        scissors: &[vk::Rect2D],
        descriptor_sets: &[vk::DescriptorSet],
        pipeline_layout: vk::PipelineLayout,
        graphic_pipeline: vk::Pipeline,
        registered_meshes: &[RegisteredMesh],
        vertex_shader_module: vk::ShaderModule,
        fragment_shader_module: vk::ShaderModule,
    ) {
        let mut metrics = Metrics::default();

        while !event_states.exiting.load(Ordering::Relaxed) {
            if event_states.minimized.load(Ordering::Relaxed) {
                continue;
            }

            metrics.start_frame();
            let delta = metrics.delta_start_to_start;

            uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, delta.as_secs_f32());
            Self::update_uniform_buffer(device, uniform_color_buffer_memory, uniform);

            let result = unsafe {
                swapchain_loader.acquire_next_image(
                    swapchain.swapchain_khr,
                    u64::MAX,
                    swapchain_resources.present_complete_semaphore,
                    vk::Fence::null(),
                )
            };
            let (present_index, _) = match result {
                Ok(result) => result,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    println!("Outdated swapchain");
                    continue;
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
                .render_pass(renderpass)
                .framebuffer(framebuffers[present_index as usize])
                .render_area(self.resolution.into())
                .clear_values(&clear_values);

            crate::vulkan::record::record_submit_commandbuffer(
                &device,
                swapchain_resources.draw_command_buffer,
                swapchain_resources.draw_commands_reuse_fence,
                swapchain.present_queue,
                &[vk::PipelineStageFlags::BOTTOM_OF_PIPE],
                &[swapchain_resources.present_complete_semaphore],
                &[swapchain_resources.rendering_complete_semaphore],
                |device, draw_command_buffer| unsafe {
                    device.cmd_begin_render_pass(
                        draw_command_buffer,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    device.cmd_bind_descriptor_sets(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline_layout,
                        0,
                        &descriptor_sets,
                        &[],
                    );
                    device.cmd_bind_pipeline(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        graphic_pipeline,
                    );
                    device.cmd_set_viewport(draw_command_buffer, 0, &viewports);
                    device.cmd_set_scissor(draw_command_buffer, 0, &scissors);

                    for registered_mesh in registered_meshes {
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
            let wait_semaphors = [swapchain_resources.rendering_complete_semaphore];
            let swapchains = [swapchain.swapchain_khr];
            let image_indices = [present_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&wait_semaphors)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let queue_present_result =
                unsafe { swapchain_loader.queue_present(swapchain.present_queue, &present_info) };

            match queue_present_result {
                Ok(_) => {}
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    println!("Outdated swapchain");
                    continue;
                }
                Err(err) => panic!("Failed to present queue: {:?}", err),
            }

            metrics.end_frame();
        }

        unsafe {
            device.device_wait_idle().unwrap();

            device.destroy_shader_module(vertex_shader_module, None);
            device.destroy_shader_module(fragment_shader_module, None);

            device.free_memory(image_buffer_memory, None);
            device.destroy_buffer(image_buffer, None);
            device.free_memory(texture_memory, None);
            device.destroy_image_view(tex_image_view, None);
            device.destroy_image(texture_image, None);

            for registered_mesh in registered_meshes.iter() {
                device.free_memory(registered_mesh.index_buffer_memory, None);
                device.destroy_buffer(registered_mesh.index_buffer, None);
                device.free_memory(registered_mesh.vertex_buffer_memory, None);
                device.destroy_buffer(registered_mesh.vertex_buffer, None);
            }

            for &descriptor_set_layout in desc_set_layouts.iter() {
                device.destroy_descriptor_set_layout(descriptor_set_layout, None);
            }
            device.destroy_descriptor_pool(descriptor_pool, None);
            device.destroy_sampler(texture_sampler, None);

            device.free_memory(uniform_color_buffer_memory, None);
            device.destroy_buffer(uniform_color_buffer, None);

            self.destroy_swapchain(
                device,
                swapchain,
                swapchain_resources,
                framebuffers,
                swapchain_loader,
            );

            for &pipeline in graphics_pipelines.iter() {
                device.destroy_pipeline(pipeline, None);
            }
            device.destroy_pipeline_layout(pipeline_layout, None);

            device.destroy_render_pass(renderpass, None);

            device.destroy_semaphore(swapchain_resources.present_complete_semaphore, None);
            device.destroy_semaphore(swapchain_resources.rendering_complete_semaphore, None);

            device.destroy_fence(swapchain_resources.draw_commands_reuse_fence, None);
            device.destroy_fence(swapchain_resources.setup_commands_reuse_fence, None);

            device.destroy_command_pool(pool, None);

            device.destroy();
        }
    }

    pub fn destroy_swapchain(
        &self,
        device: &mut ash::Device,
        swapchain: &AAASwapchain,
        swapchain_resources: &AAAResources,
        framebuffers: &[vk::Framebuffer],
        swapchain_loader: &swapchain::Device,
    ) {
        unsafe {
            device
                .device_wait_idle()
                .expect("Failed to wait for device idle");

            for &framebuffer in framebuffers.iter() {
                device.destroy_framebuffer(framebuffer, None);
            }
            for &image_view in swapchain_resources.present_image_views.iter() {
                device.destroy_image_view(image_view, None);
            }
            swapchain_loader.destroy_swapchain(swapchain.swapchain_khr, None);

            device.free_memory(swapchain_resources.depth_image_memory, None);
            device.destroy_image_view(swapchain_resources.depth_image_view, None);
            device.destroy_image(swapchain_resources.depth_image, None);
        }
    }
}
