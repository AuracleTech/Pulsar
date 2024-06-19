use crate::vulkan::{device::AAADevice, views::find_memorytype_index};
use ash::{util::Align, vk};
use std::mem;

#[derive(Clone, Debug, Copy)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug)]
pub struct RegisteredMesh {
    pub mesh: Mesh,
    pub vertex_buffer: vk::Buffer,
    pub vertex_buffer_memory: vk::DeviceMemory,
    pub index_buffer: vk::Buffer,
    pub index_buffer_memory: vk::DeviceMemory,
}

impl Mesh {
    pub fn register(
        self,
        device: &AAADevice,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> RegisteredMesh {
        unsafe {
            let index_buffer_info = vk::BufferCreateInfo::default()
                .size((self.indices.len() * mem::size_of::<u32>()) as u64)
                .usage(vk::BufferUsageFlags::INDEX_BUFFER)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let index_buffer = device.ash.create_buffer(&index_buffer_info, None).unwrap();
            let index_buffer_memory_req = device.ash.get_buffer_memory_requirements(index_buffer);
            let index_buffer_memory_index = find_memorytype_index(
                &index_buffer_memory_req,
                device_memory_properties,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
            .expect("Unable to find suitable memorytype for the index buffer.");

            let index_allocate_info = vk::MemoryAllocateInfo {
                allocation_size: index_buffer_memory_req.size,
                memory_type_index: index_buffer_memory_index,
                ..Default::default()
            };
            let index_buffer_memory = device
                .ash
                .allocate_memory(&index_allocate_info, None)
                .unwrap();
            let index_ptr = device
                .ash
                .map_memory(
                    index_buffer_memory,
                    0,
                    index_buffer_memory_req.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();
            let mut index_slice = Align::new(
                index_ptr,
                mem::align_of::<u32>() as u64,
                index_buffer_memory_req.size,
            );
            index_slice.copy_from_slice(&self.indices);
            device.ash.unmap_memory(index_buffer_memory);
            device
                .ash
                .bind_buffer_memory(index_buffer, index_buffer_memory, 0)
                .unwrap();

            let vertex_input_buffer_info = vk::BufferCreateInfo {
                size: (self.vertices.len() * mem::size_of::<Vertex>()) as u64,
                usage: vk::BufferUsageFlags::VERTEX_BUFFER,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                ..Default::default()
            };

            let vertex_input_buffer = device
                .ash
                .create_buffer(&vertex_input_buffer_info, None)
                .unwrap();

            let vertex_input_buffer_memory_req = device
                .ash
                .get_buffer_memory_requirements(vertex_input_buffer);

            let vertex_input_buffer_memory_index = find_memorytype_index(
                &vertex_input_buffer_memory_req,
                device_memory_properties,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
            .expect("Unable to find suitable memorytype for the vertex buffer.");

            let vertex_buffer_allocate_info = vk::MemoryAllocateInfo {
                allocation_size: vertex_input_buffer_memory_req.size,
                memory_type_index: vertex_input_buffer_memory_index,
                ..Default::default()
            };

            let vertex_input_buffer_memory = device
                .ash
                .allocate_memory(&vertex_buffer_allocate_info, None)
                .unwrap();

            let vert_ptr = device
                .ash
                .map_memory(
                    vertex_input_buffer_memory,
                    0,
                    vertex_input_buffer_memory_req.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();

            let mut vert_align = Align::new(
                vert_ptr,
                mem::align_of::<Vertex>() as u64,
                vertex_input_buffer_memory_req.size,
            );
            vert_align.copy_from_slice(&self.vertices);
            device.ash.unmap_memory(vertex_input_buffer_memory);
            device
                .ash
                .bind_buffer_memory(vertex_input_buffer, vertex_input_buffer_memory, 0)
                .unwrap();

            RegisteredMesh {
                mesh: self,
                vertex_buffer: vertex_input_buffer,
                vertex_buffer_memory: vertex_input_buffer_memory,
                index_buffer,
                index_buffer_memory,
            }
        }
    }
}

#[derive(Debug)]
pub struct Model {
    pub meshes: Vec<Mesh>,
}

#[derive(Debug)]
pub struct Scene {
    pub models: Vec<Model>,
}
