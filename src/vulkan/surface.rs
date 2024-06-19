use super::{device::AAADevice, surface_resources::AAAResources, AAABase};
use crate::metrics::Metrics;
use ash::{khr::surface, util::Align, vk};
use glam::Mat4;
use rwh_06::{HasDisplayHandle, HasWindowHandle};
use std::{error::Error, mem, sync::Arc};

pub struct AAASurface {
    pub surface_khr: vk::SurfaceKHR,
    pub format: vk::SurfaceFormatKHR,
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_index: u32,
    pub resources: Option<AAAResources>,
}

impl AAASurface {
    pub fn new(
        renderer: &Arc<AAABase>,
        window: &winit::window::Window,
        physical_device_list: &[ash::vk::PhysicalDevice],
    ) -> Result<Self, Box<dyn Error>> {
        let surface_khr = unsafe {
            ash_window::create_surface(
                &renderer.entry,
                &renderer.instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )
            .unwrap()
        };

        let (physical_device, queue_family_index) = physical_device_list
            .iter()
            .find_map(|physical_device| unsafe {
                renderer
                    .instance
                    .get_physical_device_queue_family_properties(*physical_device)
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        let supports_graphic_and_surface =
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                && renderer
                                    .surface_loader
                                    .get_physical_device_surface_support(
                                        *physical_device,
                                        index as u32,
                                        surface_khr,
                                    )
                                    .unwrap();
                        if supports_graphic_and_surface {
                            Some((*physical_device, index))
                        } else {
                            None
                        }
                    })
            })
            .expect("Couldn't find suitable device.");
        let queue_family_index = queue_family_index as u32;

        let format = unsafe {
            renderer
                .surface_loader
                .get_physical_device_surface_formats(physical_device, surface_khr)
                .unwrap()[0]
        };

        let capabilities = unsafe {
            renderer
                .surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface_khr)
                .unwrap()
        };

        Ok(Self {
            surface_khr,
            format,
            capabilities,
            physical_device,
            queue_family_index,
            resources: None,
        })
    }

    pub fn recreate(&mut self, surface_loader: &surface::Instance) {
        self.format = unsafe {
            surface_loader
                .get_physical_device_surface_formats(self.physical_device, self.surface_khr)
                .unwrap()[0]
        };

        self.capabilities = unsafe {
            surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface_khr)
                .unwrap()
        };
    }

    // pub fn update(&self, uniform: Mat4) {
    //     self.uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 5); // TODO reinplement delta time
    //     self.update_uniform_buffer(&self.device, self.uniform_buffer_memory, self.uniform);
    // }

    fn update_uniform_buffer(
        device: &AAADevice,
        uniform_buffer_memory: vk::DeviceMemory,
        new_transform: Mat4,
    ) {
        unsafe {
            let uniform_ptr = device
                .ash
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
            device.ash.unmap_memory(uniform_buffer_memory);
        }
    }

    pub fn render(&self, resources: &mut AAAResources, metrics: &mut Metrics) -> bool {
        metrics.start_frame();

        let force_throttle = false; // TEMP
        if force_throttle {
            std::thread::sleep(std::time::Duration::from_millis(32));
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
            resources.swapchain_loader.ash.acquire_next_image(
                resources.swapchain.swapchain_khr,
                u64::MAX,
                resources.present_complete_semaphore,
                vk::Fence::null(),
            )
        };
        let (present_index, _) = match result {
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return true,
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
            .render_pass(resources.renderpass)
            .framebuffer(resources.framebuffers[present_index as usize])
            .render_area(self.capabilities.current_extent.into())
            .clear_values(&clear_values);

        crate::vulkan::record::record_submit_commandbuffer(
            &resources.device,
            resources.draw_command_buffer,
            resources.draw_commands_reuse_fence,
            resources.swapchain.present_queue,
            &[vk::PipelineStageFlags::BOTTOM_OF_PIPE],
            &[resources.present_complete_semaphore],
            &[resources.rendering_complete_semaphore],
            |device, draw_command_buffer| unsafe {
                device.ash.cmd_begin_render_pass(
                    draw_command_buffer,
                    &render_pass_begin_info,
                    vk::SubpassContents::INLINE,
                );
                device.ash.cmd_bind_descriptor_sets(
                    draw_command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    resources.pipeline_layout,
                    0,
                    &resources.descriptor_sets,
                    &[],
                );
                device.ash.cmd_bind_pipeline(
                    draw_command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    resources.graphic_pipeline,
                );
                device
                    .ash
                    .cmd_set_viewport(draw_command_buffer, 0, &resources.viewports);
                device
                    .ash
                    .cmd_set_scissor(draw_command_buffer, 0, &resources.scissors);

                for registered_mesh in &resources.registered_meshes {
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
                        0, // TEST why was it to 1 before?
                    );
                }

                // Or draw without the index buffer
                // device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                device.ash.cmd_end_render_pass(draw_command_buffer);
            },
        );
        let wait_semaphors = [resources.rendering_complete_semaphore];
        let swapchains = [resources.swapchain.swapchain_khr];
        let image_indices = [present_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&wait_semaphors)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        let queue_present_result = unsafe {
            resources
                .swapchain_loader
                .ash
                .queue_present(resources.swapchain.present_queue, &present_info)
        };

        match queue_present_result {
            Ok(_) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return true,
            Err(err) => panic!("Failed to present queue: {:?}", err),
        }

        metrics.end_frame();
        false
    }
}
