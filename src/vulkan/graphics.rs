use ash::vk;

use super::{device::AAADevice, surface::AAASurface, surface_resources::AAAResources, AAABase};
use crate::{input_manager::EventStates, metrics::Metrics, model::mat4_to_bytes};
use std::sync::{atomic::Ordering, Arc, Mutex};

pub struct AAAGraphics {
    pub device: Arc<AAADevice>,
    pub base: Arc<AAABase>,
    pub surface: Arc<Mutex<AAASurface>>,
    pub resources: AAAResources,
    pub event_states: Arc<EventStates>,
}

impl AAAGraphics {
    pub fn new(
        base: Arc<AAABase>,
        surface: Arc<Mutex<AAASurface>>,
        event_states: Arc<EventStates>,
        width: u32,
        height: u32,
    ) -> Self {
        let resources = AAAResources::new(base.clone(), surface.clone(), width, height);
        Self {
            device: resources.device.clone(),
            base,
            surface,
            resources,
            event_states,
        }
    }

    pub fn cycle(&mut self) {
        let surface = self.surface.lock().unwrap();
        let mut metrics = Metrics::default();

        while !self.event_states.exiting.load(Ordering::Relaxed) {
            metrics.start_frame();

            // MARK: throttle
            // TEMP
            let force_throttle = false;
            let throttle_duration = std::time::Duration::from_millis(1);
            if force_throttle {
                std::thread::sleep(throttle_duration);
            }

            // MARK: rotate in real time
            // let delta = metrics.delta_start_to_start;
            // resources.uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, delta.as_secs_f32());
            // Self::update_uniform_buffer(
            //     &resources.device,
            //     resources.uniform_color_buffer_memory,
            //     resources.uniform,
            // );

            let result = unsafe {
                self.resources.swapchain_loader.ash.acquire_next_image(
                    self.resources.swapchain.swapchain_khr,
                    u64::MAX,
                    self.resources.present_complete_semaphore,
                    vk::Fence::null(),
                )
            };
            let (present_index, _) = match result {
                Ok(result) => result,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => break,
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
                .render_pass(self.resources.renderpass)
                .framebuffer(self.resources.framebuffers[present_index as usize])
                .render_area(surface.capabilities.current_extent.into())
                .clear_values(&clear_values);

            crate::vulkan::record::record_submit_commandbuffer(
                &self.resources.device,
                self.resources.draw_command_buffer,
                self.resources.draw_commands_reuse_fence,
                self.resources.swapchain.present_queue,
                &[vk::PipelineStageFlags::BOTTOM_OF_PIPE],
                &[self.resources.present_complete_semaphore],
                &[self.resources.rendering_complete_semaphore],
                |device, draw_command_buffer| unsafe {
                    device.ash.cmd_begin_render_pass(
                        draw_command_buffer,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    device.ash.cmd_bind_descriptor_sets(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.resources.pipeline_layout,
                        0,
                        &self.resources.descriptor_sets,
                        &[],
                    );
                    device.ash.cmd_bind_pipeline(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.resources.graphic_pipeline,
                    );
                    device
                        .ash
                        .cmd_set_viewport(draw_command_buffer, 0, &self.resources.viewports);
                    device
                        .ash
                        .cmd_set_scissor(draw_command_buffer, 0, &self.resources.scissors);

                    for registered_mesh in &self.resources.projection_registered_meshes {
                        let pvm = self.resources.camera.perspective.projection_view
                            * registered_mesh.mesh.transform;

                        device.ash.cmd_push_constants(
                            draw_command_buffer,
                            self.resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            mat4_to_bytes(&pvm),
                        );
                        device.ash.cmd_bind_vertex_buffers(
                            draw_command_buffer,
                            0,
                            &[registered_mesh.vertex_buffer],
                            &[0],
                        );
                        device.ash.cmd_bind_index_buffer(
                            draw_command_buffer,
                            registered_mesh.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.ash.cmd_draw_indexed(
                            draw_command_buffer,
                            registered_mesh.mesh.indices.len() as u32,
                            1,
                            0,
                            0,
                            0,
                        );
                    }

                    for registered_mesh in &self.resources.orthographic_registered_meshes {
                        let pvm = self.resources.camera.orthographic.projection_view
                            * registered_mesh.mesh.transform;

                        device.ash.cmd_push_constants(
                            draw_command_buffer,
                            self.resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            mat4_to_bytes(&pvm),
                        );
                        device.ash.cmd_bind_vertex_buffers(
                            draw_command_buffer,
                            0,
                            &[registered_mesh.vertex_buffer],
                            &[0],
                        );
                        device.ash.cmd_bind_index_buffer(
                            draw_command_buffer,
                            registered_mesh.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.ash.cmd_draw_indexed(
                            draw_command_buffer,
                            registered_mesh.mesh.indices.len() as u32,
                            1,
                            0,
                            0,
                            0,
                        );
                    }

                    // Or draw without the index buffer
                    // device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                    device.ash.cmd_end_render_pass(draw_command_buffer);
                },
            );
            let wait_semaphors = [self.resources.rendering_complete_semaphore];
            let swapchains = [self.resources.swapchain.swapchain_khr];
            let image_indices = [present_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&wait_semaphors)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            let queue_present_result = unsafe {
                self.resources
                    .swapchain_loader
                    .ash
                    .queue_present(self.resources.swapchain.present_queue, &present_info)
            };

            match queue_present_result {
                Ok(_) => {}
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => break,
                Err(err) => panic!("Failed to present queue: {:?}", err),
            }

            metrics.end_frame();
        }
    }

    pub fn recreate_swapchain(&mut self, width: u32, height: u32) {
        self.destroy_swapchain();

        let mut surface = self.surface.lock().unwrap();
        surface.recreate(&*self.base.surface_loader);
        self.resources.recreate_viewports(width, height); // TODO sync release with drop
        self.resources.recreate_scissors(width, height); // TODO sync release with drop

        self.resources.swapchain = crate::vulkan::swapchain::AAASwapchain::new(
            &self.resources.device,
            &self.base,
            &surface,
            surface.physical_device,
            surface.queue_family_index,
            width,
            height,
            &self.resources.swapchain_loader,
        );

        // MARK: recreate_views_and_depth
        let (
            present_images,
            present_image_views,
            depth_image_view,
            depth_image,
            depth_image_memory,
            device_memory_properties_new,
        ) = crate::vulkan::views::create_views_and_depth(
            &self.resources.device,
            &self.base,
            &self.resources.swapchain,
            &surface,
            &surface.physical_device,
            &self.resources.swapchain_loader,
        );

        self.resources.present_images = present_images;
        self.resources.present_image_views = present_image_views;
        self.resources.depth_image_view = depth_image_view;
        self.resources.depth_image = depth_image;
        self.resources.depth_image_memory = depth_image_memory;
        self.resources.device_memory_properties = device_memory_properties_new;

        // MARK: recreate_framebuffers
        self.resources.framebuffers = crate::vulkan::framebuffer::create_framebuffers(
            &self.resources.device,
            &surface,
            &self.resources.present_image_views,
            depth_image_view,
            self.resources.renderpass,
        )
        .unwrap();

        self.resources.register_depth_image_memory();

        self.resources.camera.perspective.aspect_ratio = width as f32 / height as f32;
        self.resources.camera.orthographic.right = width as f32;
        self.resources.camera.orthographic.top = height as f32;
        self.resources.camera.update();
    }

    pub fn destroy_swapchain(&self) {
        unsafe {
            self.resources.device.ash.device_wait_idle().unwrap();

            for &framebuffer in self.resources.framebuffers.iter() {
                self.resources
                    .device
                    .ash
                    .destroy_framebuffer(framebuffer, None);
            }
            for &image_view in self.resources.present_image_views.iter() {
                self.resources
                    .device
                    .ash
                    .destroy_image_view(image_view, None);
            }
            self.resources
                .swapchain_loader
                .ash
                .destroy_swapchain(self.resources.swapchain.swapchain_khr, None);

            self.resources
                .device
                .ash
                .free_memory(self.resources.depth_image_memory, None);
            self.resources
                .device
                .ash
                .destroy_image_view(self.resources.depth_image_view, None);
            self.resources
                .device
                .ash
                .destroy_image(self.resources.depth_image, None);
        }
    }
}

impl Drop for AAAGraphics {
    fn drop(&mut self) {
        self.destroy_swapchain();

        unsafe {
            for &pipeline in self.resources.graphics_pipelines.iter() {
                self.resources.device.ash.destroy_pipeline(pipeline, None);
            }

            self.resources
                .device
                .ash
                .destroy_pipeline_layout(self.resources.pipeline_layout, None);

            self.resources
                .device
                .ash
                .destroy_render_pass(self.resources.renderpass, None);

            self.resources
                .device
                .ash
                .destroy_semaphore(self.resources.present_complete_semaphore, None);
            self.resources
                .device
                .ash
                .destroy_semaphore(self.resources.rendering_complete_semaphore, None);

            self.resources
                .device
                .ash
                .destroy_fence(self.resources.draw_commands_reuse_fence, None);
            self.resources
                .device
                .ash
                .destroy_fence(self.resources.setup_commands_reuse_fence, None);

            self.resources
                .device
                .ash
                .destroy_command_pool(self.resources.pool, None);
        }
    }
}
