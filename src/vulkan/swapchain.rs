use super::surface::AAASurface;
use ash::{
    khr::{surface, swapchain},
    vk, Device,
};
use std::error::Error;

pub struct AAASwapchain {
    pub swapchain_khr: vk::SwapchainKHR,
    pub _desired_image_count: u32,
    pub _present_mode: vk::PresentModeKHR,
    pub present_queue: vk::Queue,
}

impl AAASwapchain {
    pub fn new(
        device: &Device,
        surface_loader: &surface::Instance,
        surface: &AAASurface,
        pdevice: vk::PhysicalDevice,
        queue_family_index: u32,
        width: u32,
        height: u32,
        swapchain_loader: &swapchain::Device,
    ) -> Result<AAASwapchain, Box<dyn Error>> {
        let present_modes = unsafe {
            surface_loader
                .get_physical_device_surface_present_modes(pdevice, surface.surface_khr)
                .unwrap()
        };
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let present_queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let mut desired_image_count = surface.capabilities.min_image_count + 1;
        if surface.capabilities.max_image_count > 0
            && desired_image_count > surface.capabilities.max_image_count
        {
            desired_image_count = surface.capabilities.max_image_count;
        }
        let surface_resolution = match surface.capabilities.current_extent.width {
            u32::MAX => vk::Extent2D { width, height },
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

        let swapchain = unsafe {
            swapchain_loader
                .create_swapchain(&swapchain_create_info, None)
                .unwrap()
        };

        Ok(AAASwapchain {
            swapchain_khr: swapchain,
            _desired_image_count: desired_image_count,
            _present_mode: present_mode,
            present_queue,
        })
    }
}
