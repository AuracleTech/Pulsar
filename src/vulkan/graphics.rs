use super::{surface::AAASurface, surface_resources::AAAResources, AAABase};
use crate::{input_manager::EventStates, metrics::Metrics};
use std::sync::{atomic::Ordering, Arc, Mutex};

pub struct AAAGraphics {
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
    ) -> Self {
        let resources = AAAResources::new(base.clone(), surface.clone());
        Self {
            base,
            surface,
            resources,
            event_states,
        }
    }

    pub fn cycle(&self) {
        let surface = self.surface.lock().unwrap();
        let mut metrics = Metrics::default();

        while !self.event_states.exiting.load(Ordering::Relaxed) {
            surface.render(&self.resources, &mut metrics);
        }
    }

    // TEMP for now only 1
    // pub fn recreate_swapchain(&mut self) {
    // 	self.destroy_swapchain();
    // 	self.resources = AAAResources::new(&self.base, &self.surface);
    // }

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

        // TEMP for now only 1
        unsafe {
            self.resources
                .device
                .ash
                .destroy_pipeline(self.resources.graphic_pipeline, None);

            // for &pipeline in self.resources.graphics_pipelines.iter() {
            //     self.device.ash.destroy_pipeline(pipeline, None);
            // }

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
