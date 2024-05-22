use super::device::AAADevice;
use ash::vk;
use std::error::Error;

pub fn create_command_pools(
    device: &AAADevice,
    queue_family_index: u32,
) -> Result<vk::CommandPool, Box<dyn Error>> {
    let pool_create_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(queue_family_index);

    let pool = unsafe {
        device
            .ash
            .create_command_pool(&pool_create_info, None)
            .unwrap()
    };

    Ok(pool)
}
