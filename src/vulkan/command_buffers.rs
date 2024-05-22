use super::device::AAADevice;
use ash::vk;
use std::error::Error;

pub fn create_command_buffers(
    device: &AAADevice,
    pool: vk::CommandPool,
) -> Result<(vk::CommandBuffer, vk::CommandBuffer), Box<dyn Error>> {
    let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_buffer_count(2)
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY);

    let command_buffers = unsafe {
        device
            .ash
            .allocate_command_buffers(&command_buffer_allocate_info)
            .unwrap()
    };
    let setup_command_buffer = command_buffers[0];
    let draw_command_buffer = command_buffers[1];

    Ok((setup_command_buffer, draw_command_buffer))
}
