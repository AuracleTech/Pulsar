use super::views::find_memorytype_index;
use ash::{util::Align, vk, Device};
use glam::Mat4;
use std::mem;

pub fn create_uniform_buffer(
    device: &Device,
    device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    uniform: Mat4,
) -> (vk::Buffer, vk::DeviceMemory) {
    let uniform_buffer_info = vk::BufferCreateInfo {
        size: mem::size_of_val(&uniform) as u64,
        usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        ..Default::default()
    };
    unsafe {
        let uniform_buffer = device.create_buffer(&uniform_buffer_info, None).unwrap();
        let uniform_buffer_memory_req = device.get_buffer_memory_requirements(uniform_buffer);
        let uniform_buffer_memory_index = find_memorytype_index(
            &uniform_buffer_memory_req,
            device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the vertex buffer.");

        let uniform_buffer_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: uniform_buffer_memory_req.size,
            memory_type_index: uniform_buffer_memory_index,
            ..Default::default()
        };
        let uniform_buffer_memory = device
            .allocate_memory(&uniform_buffer_allocate_info, None)
            .unwrap();
        let uniform_ptr = device
            .map_memory(
                uniform_buffer_memory,
                0,
                uniform_buffer_memory_req.size,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let mut uniform_aligned_slice = Align::new(
            uniform_ptr,
            mem::align_of::<Mat4>() as u64,
            uniform_buffer_memory_req.size,
        );
        uniform_aligned_slice.copy_from_slice(&[uniform]);
        device.unmap_memory(uniform_buffer_memory);
        device
            .bind_buffer_memory(uniform_buffer, uniform_buffer_memory, 0)
            .unwrap();

        (uniform_buffer, uniform_buffer_memory)
    }
}
