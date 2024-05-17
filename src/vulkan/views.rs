use super::{surface::AAASurface, swapchain::AAASwapchain};
use ash::{khr::swapchain, vk, Device};

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

pub fn create_views_and_depth(
    device: &Device,
    instance: &ash::Instance,
    swapchain: &AAASwapchain,
    surface: &AAASurface,
    pdevice: &vk::PhysicalDevice,
    swapchain_loader: &swapchain::Device,
) -> (
    Vec<ash::vk::Image>,
    Vec<vk::ImageView>,
    vk::ImageView,
    vk::Image,
    vk::DeviceMemory,
    vk::PhysicalDeviceMemoryProperties,
) {
    let present_images = unsafe {
        swapchain_loader
            .get_swapchain_images(swapchain.swapchain_khr)
            .unwrap()
    };
    let present_image_views: Vec<vk::ImageView> = present_images
        .iter()
        .map(|&image| {
            let create_view_info = vk::ImageViewCreateInfo::default()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface.format.format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image);
            unsafe { device.create_image_view(&create_view_info, None).unwrap() }
        })
        .collect();
    let device_memory_properties =
        unsafe { instance.get_physical_device_memory_properties(*pdevice) };
    let depth_image_create_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::D16_UNORM)
        .extent(surface.resolution.into())
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let depth_image = unsafe { device.create_image(&depth_image_create_info, None).unwrap() };
    let depth_image_memory_req = unsafe { device.get_image_memory_requirements(depth_image) };
    let depth_image_memory_index = find_memorytype_index(
        &depth_image_memory_req,
        &device_memory_properties,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .expect("Unable to find suitable memory index for depth image.");

    let depth_image_allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(depth_image_memory_req.size)
        .memory_type_index(depth_image_memory_index);

    let depth_image_memory = unsafe {
        device
            .allocate_memory(&depth_image_allocate_info, None)
            .unwrap()
    };

    unsafe {
        device
            .bind_image_memory(depth_image, depth_image_memory, 0)
            .expect("Unable to bind depth image memory")
    };

    let depth_image_view_info = vk::ImageViewCreateInfo::default()
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                .level_count(1)
                .layer_count(1),
        )
        .image(depth_image)
        .format(depth_image_create_info.format)
        .view_type(vk::ImageViewType::TYPE_2D);

    let depth_image_view = unsafe {
        device
            .create_image_view(&depth_image_view_info, None)
            .unwrap()
    };

    (
        present_images,
        present_image_views,
        depth_image_view,
        depth_image,
        depth_image_memory,
        device_memory_properties,
    )
}
