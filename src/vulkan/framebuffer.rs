use super::surface::AAASurface;
use ash::{vk, Device};
use std::error::Error;

pub fn create_framebuffers(
    device: &Device,
    surface: &AAASurface,
    present_image_views: &[vk::ImageView],
    depth_image_view: vk::ImageView,
    renderpass: vk::RenderPass,
) -> Result<Vec<vk::Framebuffer>, Box<dyn Error>> {
    let framebuffers: Vec<vk::Framebuffer> = present_image_views
        .iter()
        .map(|&present_image_view| {
            let framebuffer_attachments = [present_image_view, depth_image_view];
            let frame_buffer_create_info = vk::FramebufferCreateInfo::default()
                .render_pass(renderpass)
                .attachments(&framebuffer_attachments)
                .width(surface.capabilities.current_extent.width)
                .height(surface.capabilities.current_extent.height)
                .layers(1);

            unsafe {
                device
                    .create_framebuffer(&frame_buffer_create_info, None)
                    .unwrap()
            }
        })
        .collect();

    Ok(framebuffers)
}
