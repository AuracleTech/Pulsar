use super::{
    device::AAADevice,
    record::record_submit_commandbuffer,
    surface::AAASurface,
    swapchain::{AAASwapchain, AAASwapchainLoader},
    views::find_memorytype_index,
    AAABase,
};
use crate::model::{Mesh, RegisteredMesh, Vertex};
use ash::{
    util::Align,
    vk::{self, DescriptorSetLayout},
};
use glam::Mat4;
use std::{
    mem,
    sync::{Arc, Mutex},
};

pub struct AAAResources {
    pub device: Arc<AAADevice>, // TEMP should be in super, everything uses it

    pub draw_command_buffer: vk::CommandBuffer,
    pub setup_command_buffer: vk::CommandBuffer,

    pub depth_image: vk::Image,
    pub depth_image_view: vk::ImageView,
    pub depth_image_memory: vk::DeviceMemory,

    pub present_images: Vec<vk::Image>,
    pub present_image_views: Vec<vk::ImageView>,

    pub draw_commands_reuse_fence: vk::Fence,
    pub setup_commands_reuse_fence: vk::Fence,

    pub present_complete_semaphore: vk::Semaphore,
    pub rendering_complete_semaphore: vk::Semaphore,

    pub vertex_shader_module: vk::ShaderModule,
    pub fragment_shader_module: vk::ShaderModule,

    pub image_buffer_memory: vk::DeviceMemory,
    pub image_buffer: vk::Buffer,
    pub texture_memory: vk::DeviceMemory,
    pub tex_image_view: vk::ImageView,
    pub texture_image: vk::Image,

    pub desc_set_layouts: [DescriptorSetLayout; 1],
    pub descriptor_pool: vk::DescriptorPool,
    pub texture_sampler: vk::Sampler,

    pub uniform_color_buffer_memory: vk::DeviceMemory,
    pub uniform_color_buffer: vk::Buffer,
    pub graphics_pipelines: Vec<vk::Pipeline>,
    pub pipeline_layout: vk::PipelineLayout,
    pub renderpass: vk::RenderPass,
    pub pool: vk::CommandPool,

    pub uniform: Mat4,

    pub swapchain_loader: AAASwapchainLoader,
    pub swapchain: AAASwapchain,

    pub framebuffers: Vec<vk::Framebuffer>,
    pub viewports: [vk::Viewport; 1],
    pub scissors: [vk::Rect2D; 1],

    pub descriptor_sets: Vec<vk::DescriptorSet>,
    pub graphic_pipeline: vk::Pipeline,

    pub registered_meshes: Vec<RegisteredMesh>,
    pub device_memory_properties: vk::PhysicalDeviceMemoryProperties,
}

impl AAAResources {
    pub fn new(
        base: Arc<AAABase>,
        surface: Arc<Mutex<AAASurface>>,
        width: u32,
        height: u32,
    ) -> Self {
        let surface = surface.lock().unwrap();

        let device = AAADevice::new(
            &base.instance,
            surface.physical_device,
            surface.queue_family_index,
        );

        let swapchain_loader = AAASwapchainLoader::new(&base, &device);

        // TODO get from os window api for linux and possibly more
        // let size = surface.capabilities.current_extent; // TODO VERIFY THERE S NOT MORE ELSEWHERE

        let swapchain = AAASwapchain::new(
            &device,
            &base,
            &surface,
            surface.physical_device,
            surface.queue_family_index,
            width,
            height,
            &swapchain_loader,
        );

        let (draw_commands_reuse_fence, setup_commands_reuse_fence) =
            crate::vulkan::fence_semaphores::create_fences(&device).unwrap();

        let (
            present_images,
            present_image_views,
            depth_image_view,
            depth_image,
            depth_image_memory,
            device_memory_properties,
        ) = crate::vulkan::views::create_views_and_depth(
            &device,
            &base,
            &swapchain,
            &surface,
            &surface.physical_device,
            &swapchain_loader,
        );

        let (present_complete_semaphore, rendering_complete_semaphore) =
            crate::vulkan::fence_semaphores::create_semaphores(&device).unwrap();

        let renderpass = crate::vulkan::renderpass::create_renderpass(&surface, &device).unwrap();

        let (descriptor_pool, descriptor_sets, desc_set_layouts) =
            crate::vulkan::descriptor_set::create_descriptor_set(&device);

        let (
            graphic_pipeline,
            viewports,
            scissors,
            graphics_pipelines,
            pipeline_layout,
            vertex_shader_module,
            fragment_shader_module,
        ) = crate::vulkan::pipeline::create_pipeline(
            &device,
            &surface,
            renderpass,
            desc_set_layouts,
        );

        let framebuffers = crate::vulkan::framebuffer::create_framebuffers(
            &device,
            &surface,
            &present_image_views,
            depth_image_view,
            renderpass,
        )
        .unwrap();

        let pool =
            crate::vulkan::command_pools::create_command_pools(&device, surface.queue_family_index)
                .unwrap();

        let (setup_command_buffer, draw_command_buffer) =
            crate::vulkan::command_buffers::create_command_buffers(&device, pool).unwrap();

        crate::vulkan::record::record_submit_commandbuffer(
            &device,
            setup_command_buffer,
            setup_commands_reuse_fence,
            swapchain.present_queue,
            &[],
            &[],
            &[],
            |device, setup_command_buffer| {
                let layout_transition_barriers = vk::ImageMemoryBarrier::default()
                    .image(depth_image)
                    .dst_access_mask(
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    )
                    .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .layer_count(1)
                            .level_count(1),
                    );

                unsafe {
                    device.ash.cmd_pipeline_barrier(
                        setup_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[layout_transition_barriers],
                    )
                };
            },
        );

        // MARK: UNIFORM BUFFER
        let uniform = Mat4::IDENTITY;
        // TEMP: rotate UBO transfrom by 25% of PI
        // uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, std::f32::consts::PI / 4.0);

        let (uniform_color_buffer, uniform_color_buffer_memory) =
            crate::vulkan::uniform::create_uniform_buffer(
                &device,
                &device_memory_properties,
                uniform,
            );

        // MARK: IMAGE
        let image = image::load_from_memory(include_bytes!("../../assets/img/picture.png"))
            .unwrap()
            .to_rgba8();
        let (width, height) = image.dimensions();
        let image_extent = vk::Extent2D { width, height };
        let image_data = image.into_raw();
        let image_buffer_info = vk::BufferCreateInfo {
            size: (mem::size_of::<u8>() * image_data.len()) as u64,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let image_buffer = unsafe { device.ash.create_buffer(&image_buffer_info, None).unwrap() };
        let image_buffer_memory_req =
            unsafe { device.ash.get_buffer_memory_requirements(image_buffer) };
        let image_buffer_memory_index = find_memorytype_index(
            &image_buffer_memory_req,
            &device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the image buffer.");

        let image_buffer_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: image_buffer_memory_req.size,
            memory_type_index: image_buffer_memory_index,
            ..Default::default()
        };
        let image_buffer_memory = unsafe {
            device
                .ash
                .allocate_memory(&image_buffer_allocate_info, None)
                .unwrap()
        };
        let image_ptr = unsafe {
            device
                .ash
                .map_memory(
                    image_buffer_memory,
                    0,
                    image_buffer_memory_req.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap()
        };
        let mut image_slice = unsafe {
            Align::new(
                image_ptr,
                mem::align_of::<u8>() as u64,
                image_buffer_memory_req.size,
            )
        };
        image_slice.copy_from_slice(&image_data);
        unsafe {
            device.ash.unmap_memory(image_buffer_memory);
            device
                .ash
                .bind_buffer_memory(image_buffer, image_buffer_memory, 0)
                .unwrap();
        }

        // MARK: TEXTURE
        let texture_create_info = vk::ImageCreateInfo {
            image_type: vk::ImageType::TYPE_2D,
            format: vk::Format::R8G8B8A8_UNORM,
            extent: image_extent.into(),
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let texture_image = unsafe { device.ash.create_image(&texture_create_info, None).unwrap() };
        let texture_memory_req = unsafe { device.ash.get_image_memory_requirements(texture_image) };
        let texture_memory_index = find_memorytype_index(
            &texture_memory_req,
            &device_memory_properties,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .expect("Unable to find suitable memory index for depth image.");

        let texture_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: texture_memory_req.size,
            memory_type_index: texture_memory_index,
            ..Default::default()
        };
        let texture_memory = unsafe {
            device
                .ash
                .allocate_memory(&texture_allocate_info, None)
                .unwrap()
        };
        unsafe {
            device
                .ash
                .bind_image_memory(texture_image, texture_memory, 0)
                .expect("Unable to bind depth image memory")
        };

        // MARK: REC TEXTURE
        // record_submit_commandbuffer(
        //     &device,
        //     setup_command_buffer,
        //     setup_commands_reuse_fence,
        //     swapchain.present_queue,
        //     &[],
        //     &[],
        //     &[],
        //     |device, texture_command_buffer| {
        //         let texture_barrier = vk::ImageMemoryBarrier {
        //             dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
        //             new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        //             image: texture_image,
        //             subresource_range: vk::ImageSubresourceRange {
        //                 aspect_mask: vk::ImageAspectFlags::COLOR,
        //                 level_count: 1,
        //                 layer_count: 1,
        //                 ..Default::default()
        //             },
        //             ..Default::default()
        //         };
        //         unsafe {
        //             device.ash.cmd_pipeline_barrier(
        //                 texture_command_buffer,
        //                 vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        //                 vk::PipelineStageFlags::TRANSFER,
        //                 vk::DependencyFlags::empty(),
        //                 &[],
        //                 &[],
        //                 &[texture_barrier],
        //             )
        //         };
        //         let buffer_copy_regions = vk::BufferImageCopy::default()
        //             .image_subresource(
        //                 vk::ImageSubresourceLayers::default()
        //                     .aspect_mask(vk::ImageAspectFlags::COLOR)
        //                     .layer_count(1),
        //             )
        //             .image_extent(image_extent.into());

        //         unsafe {
        //             device.ash.cmd_copy_buffer_to_image(
        //                 texture_command_buffer,
        //                 image_buffer,
        //                 texture_image,
        //                 vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        //                 &[buffer_copy_regions],
        //             )
        //         };
        //         let texture_barrier_end = vk::ImageMemoryBarrier {
        //             src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
        //             dst_access_mask: vk::AccessFlags::SHADER_READ,
        //             old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        //             new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        //             image: texture_image,
        //             subresource_range: vk::ImageSubresourceRange {
        //                 aspect_mask: vk::ImageAspectFlags::COLOR,
        //                 level_count: 1,
        //                 layer_count: 1,
        //                 ..Default::default()
        //             },
        //             ..Default::default()
        //         };
        //         unsafe {
        //             device.ash.cmd_pipeline_barrier(
        //                 texture_command_buffer,
        //                 vk::PipelineStageFlags::TRANSFER,
        //                 vk::PipelineStageFlags::FRAGMENT_SHADER,
        //                 vk::DependencyFlags::empty(),
        //                 &[],
        //                 &[],
        //                 &[texture_barrier_end],
        //             )
        //         };
        //     },
        // );

        // MARK: SAMPLER
        let sampler_info = vk::SamplerCreateInfo {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            mipmap_mode: vk::SamplerMipmapMode::LINEAR,
            address_mode_u: vk::SamplerAddressMode::MIRRORED_REPEAT,
            address_mode_v: vk::SamplerAddressMode::MIRRORED_REPEAT,
            address_mode_w: vk::SamplerAddressMode::MIRRORED_REPEAT,
            max_anisotropy: 1.0,
            border_color: vk::BorderColor::FLOAT_OPAQUE_WHITE,
            compare_op: vk::CompareOp::NEVER,
            ..Default::default()
        };

        let texture_sampler = unsafe { device.ash.create_sampler(&sampler_info, None).unwrap() };

        // MARK: TEXTURE VIEW
        let tex_image_view_info = vk::ImageViewCreateInfo {
            view_type: vk::ImageViewType::TYPE_2D,
            format: texture_create_info.format,
            components: vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A,
            },
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                layer_count: 1,
                ..Default::default()
            },
            image: texture_image,
            ..Default::default()
        };
        let tex_image_view = unsafe {
            device
                .ash
                .create_image_view(&tex_image_view_info, None)
                .unwrap()
        };

        let uniform_color_buffer_descriptor = vk::DescriptorBufferInfo {
            buffer: uniform_color_buffer,
            offset: 0,
            range: mem::size_of_val(&uniform) as u64,
        };

        let tex_descriptor = vk::DescriptorImageInfo {
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            image_view: tex_image_view,
            sampler: texture_sampler,
        };

        let write_desc_sets = [
            vk::WriteDescriptorSet {
                dst_set: descriptor_sets[0],
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                p_buffer_info: &uniform_color_buffer_descriptor,
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: descriptor_sets[0],
                dst_binding: 1,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                p_image_info: &tex_descriptor, 
                ..Default::default()
            },
        ];
        unsafe { device.ash.update_descriptor_sets(&write_desc_sets, &[]) };

        // MARK: MESHES
        let mut registered_meshes = Vec::new();

        // use rand::Rng;
        // let mut rng = rand::thread_rng();
        // for _ in 0..5 {
        //     let mut vertices = Vec::new();
        //     let mut indices = Vec::new();
        //     for _ in 0..10 {
        //         let x = rng.gen_range(-1.0..1.0);
        //         let y = rng.gen_range(-1.0..1.0);

        //         vertices.extend(
        //             [
        //                 Vertex {
        //                     pos: [x, y, 1.0, 1.0],
        //                     uv: [0.0, 0.0],
        //                 },
        //                 Vertex {
        //                     pos: [x + 0.1, y, 1.0, 1.0],
        //                     uv: [0.0, 1.0],
        //                 },
        //                 Vertex {
        //                     pos: [x + 0.1, y - 0.1, 1.0, 1.0],
        //                     uv: [1.0, 1.0],
        //                 },
        //                 Vertex {
        //                     pos: [x, y - 0.1, 1.0, 1.0],
        //                     uv: [1.0, 0.0],
        //                 },
        //             ]
        //             .iter(),
        //         );

        //         let offset = vertices.len() as u32 - 4;
        //         let quad_indices = vec![
        //             offset,
        //             offset + 1,
        //             offset + 2,
        //             offset,
        //             offset + 2,
        //             offset + 3,
        //         ];

        //         indices.extend(quad_indices);
        //     }
        //     let mesh = Mesh { vertices, indices };
        //     let registered_mesh = mesh.register(&device, &device_memory_properties);
        //     registered_meshes.push(registered_mesh);
        // }

        // MARK: LEFT_SCREEN_COVER
        let left_cover_color = [0.08627450980392157, 0.08627450980392157, 0.13333333333333333, 1.0];
        let left_cover = Mesh {
            vertices: vec![
                Vertex {
                    pos: [-1.0, -1.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                    color: left_cover_color,
                },
                Vertex {
                    pos: [-1.0, 1.0, 0.0, 1.0],
                    uv: [0.0, 1.0],
                    color: left_cover_color,
                },
                Vertex {
                    pos: [0.0, 1.0, 0.0, 1.0],
                    uv: [1.0, 1.0],
                    color: left_cover_color,
                },
                Vertex {
                    pos: [0.0, -1.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                    color: left_cover_color,
                },
            ],
            indices: vec![0u32, 1, 2, 2, 3, 0],
        };
        let registered_square = left_cover.register(&device, &device_memory_properties);
        registered_meshes.push(registered_square);
        // MARK: RIGHT_SCREEN_COVER
        let right_cover_color = [0.13333333333333333, 0.13333333333333333, 0.21176470588235294, 1.0];
        let right_cover = Mesh {
            vertices: vec![
                Vertex {
                    pos: [0.0, -1.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                    color: right_cover_color,
                },
                Vertex {
                    pos: [0.0, 1.0, 0.0, 1.0],
                    uv: [0.0, 1.0],
                    color: right_cover_color,
                },
                Vertex {
                    pos: [1.0, 1.0, 0.0, 1.0],
                    uv: [1.0, 1.0],
                    color: right_cover_color,
                },
                Vertex {
                    pos: [1.0, -1.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                    color: right_cover_color,
                },
            ],
            indices: vec![0u32, 1, 2, 2, 3, 0],
        };
        let registered_square = right_cover.register(&device, &device_memory_properties);
        registered_meshes.push(registered_square);

        // MARK: SQUARE
        // let square = Mesh {
        //     vertices: vec![
        //         Vertex {
        //             pos: [-1.0, -1.0, 0.0, 1.0],
        //             uv: [0.0, 0.0],
        //         },
        //         Vertex {
        //             pos: [-1.0, 1.0, 0.0, 1.0],
        //             uv: [0.0, 1.0],
        //         },
        //         Vertex {
        //             pos: [1.0, 1.0, 0.0, 1.0],
        //             uv: [1.0, 1.0],
        //         },
        //         Vertex {
        //             pos: [1.0, -1.0, 0.0, 1.0],
        //             uv: [1.0, 0.0],
        //         },
        //     ],
        //     indices: vec![0u32, 1, 2, 2, 3, 0],
        // };
        // let registered_square = square.register(&device, &device_memory_properties);
        // registered_meshes.push(registered_square);

        Self {
            device: Arc::new(device),

            draw_command_buffer,
            setup_command_buffer,

            depth_image,
            depth_image_view,
            depth_image_memory,

            present_images,
            present_image_views,

            draw_commands_reuse_fence,
            setup_commands_reuse_fence,

            present_complete_semaphore,
            rendering_complete_semaphore,

            vertex_shader_module,
            fragment_shader_module,

            image_buffer_memory,
            image_buffer,
            texture_memory,
            tex_image_view,
            texture_image,

            desc_set_layouts,
            descriptor_pool,
            texture_sampler,

            uniform_color_buffer_memory,
            uniform_color_buffer,
            graphics_pipelines,
            pipeline_layout,
            renderpass,
            pool,

            uniform,

            swapchain_loader,
            swapchain,

            framebuffers,
            viewports,
            scissors,

            descriptor_sets,
            graphic_pipeline,

            registered_meshes,

            device_memory_properties,
        }
    }

    // TODO reuse at creation and recreation
    pub fn recreate_viewports(&mut self, width: u32, height: u32) {
        self.viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
    }

    // TODO reuse at creation and recreation
    pub fn recreate_scissors(&mut self, width: u32, height: u32) {
        self.scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        }];
    }

    // TODO on creation also register the depth image memory instead of code dupe
    pub fn register_depth_image_memory(&mut self) {
        record_submit_commandbuffer(
            &self.device,
            self.setup_command_buffer,
            self.setup_commands_reuse_fence,
            self.swapchain.present_queue,
            &[],
            &[],
            &[],
            |_device, setup_command_buffer| {
                let layout_transition_barriers = vk::ImageMemoryBarrier::default()
                    .image(self.depth_image)
                    .dst_access_mask(
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    )
                    .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .layer_count(1)
                            .level_count(1),
                    );

                unsafe {
                    self.device.ash.cmd_pipeline_barrier(
                        setup_command_buffer,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[layout_transition_barriers],
                    );
                }
            },
        );
    }
}

impl Drop for AAAResources {
    fn drop(&mut self) {
        unsafe {
            self.device.ash.device_wait_idle().unwrap();

            self.device
                .ash
                .destroy_shader_module(self.vertex_shader_module, None);
            self.device
                .ash
                .destroy_shader_module(self.fragment_shader_module, None);

            self.device.ash.free_memory(self.image_buffer_memory, None);
            self.device.ash.destroy_buffer(self.image_buffer, None);
            self.device.ash.free_memory(self.texture_memory, None);
            self.device
                .ash
                .destroy_image_view(self.tex_image_view, None);
            self.device.ash.destroy_image(self.texture_image, None);

            for registered_mesh in self.registered_meshes.iter() {
                self.device
                    .ash
                    .free_memory(registered_mesh.index_buffer_memory, None);
                self.device
                    .ash
                    .destroy_buffer(registered_mesh.index_buffer, None);
                self.device
                    .ash
                    .free_memory(registered_mesh.vertex_buffer_memory, None);
                self.device
                    .ash
                    .destroy_buffer(registered_mesh.vertex_buffer, None);
            }

            for &descriptor_set_layout in self.desc_set_layouts.iter() {
                self.device
                    .ash
                    .destroy_descriptor_set_layout(descriptor_set_layout, None);
            }
            self.device
                .ash
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.ash.destroy_sampler(self.texture_sampler, None);

            self.device
                .ash
                .free_memory(self.uniform_color_buffer_memory, None);
            self.device
                .ash
                .destroy_buffer(self.uniform_color_buffer, None);
        }
    }
}
