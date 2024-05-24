use super::{device::AAADevice, surface::AAASurface, surface_resources::AAAResources, AAABase};
use crate::{input_manager::EventStates, metrics::Metrics};
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
            if surface.render(&mut self.resources, &mut metrics) {
                break;
            }
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
