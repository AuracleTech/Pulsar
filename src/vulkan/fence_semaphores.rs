use super::device::AAADevice;
use ash::vk;
use std::error::Error;

pub fn create_fences(device: &AAADevice) -> Result<(vk::Fence, vk::Fence), Box<dyn Error>> {
    let fence_create_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let draw_commands_reuse_fence = unsafe {
        device
            .ash
            .create_fence(&fence_create_info, None)
            .expect("Create fence failed.")
    };
    let setup_commands_reuse_fence = unsafe {
        device
            .ash
            .create_fence(&fence_create_info, None)
            .expect("Create fence failed.")
    };

    Ok((draw_commands_reuse_fence, setup_commands_reuse_fence))
}

pub fn create_semaphores(
    device: &AAADevice,
) -> Result<(vk::Semaphore, vk::Semaphore), Box<dyn Error>> {
    let semaphore_create_info = vk::SemaphoreCreateInfo::default();

    let present_complete_semaphore = unsafe {
        device
            .ash
            .create_semaphore(&semaphore_create_info, None)
            .unwrap()
    };
    let rendering_complete_semaphore = unsafe {
        device
            .ash
            .create_semaphore(&semaphore_create_info, None)
            .unwrap()
    };

    Ok((present_complete_semaphore, rendering_complete_semaphore))
}
