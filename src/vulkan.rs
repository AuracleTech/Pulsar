use std::sync::Arc;

pub mod command_buffers;
pub mod command_pools;
#[cfg(debug_assertions)]
pub mod debug_callback;
pub mod descriptor_set;
pub mod device;
pub mod fence_semaphores;
pub mod framebuffer;
pub mod graphics;
pub mod instance;
pub mod pipeline;
pub mod record;
pub mod renderpass;
pub mod surface;
pub mod surface_resources;
pub mod swapchain;
pub mod uniform;
pub mod views;

// TODO check sa many things that can be made Rc instead of Arc
pub struct AAABase {
    pub entry: ash::Entry,
    pub instance: Arc<ash::Instance>,
    pub surface_loader: Arc<ash::khr::surface::Instance>,
}

impl Drop for AAABase {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}
