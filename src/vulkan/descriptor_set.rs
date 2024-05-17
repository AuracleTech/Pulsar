use ash::{vk, Device};

pub fn create_descriptor_set(
    device: &Device,
) -> (
    vk::DescriptorPool,
    Vec<vk::DescriptorSet>,
    [vk::DescriptorSetLayout; 1],
) {
    let descriptor_sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        },
    ];
    let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(&descriptor_sizes)
        .max_sets(1);

    let descriptor_pool = unsafe {
        device
            .create_descriptor_pool(&descriptor_pool_info, None)
            .unwrap()
    };
    let desc_layout_bindings = [
        vk::DescriptorSetLayoutBinding {
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::VERTEX,
            ..Default::default()
        },
        vk::DescriptorSetLayoutBinding {
            binding: 1,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
    ];
    let descriptor_info =
        vk::DescriptorSetLayoutCreateInfo::default().bindings(&desc_layout_bindings);

    let desc_set_layouts = [unsafe {
        device
            .create_descriptor_set_layout(&descriptor_info, None)
            .unwrap()
    }];

    let desc_alloc_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool)
        .set_layouts(&desc_set_layouts);
    let descriptor_sets = unsafe { device.allocate_descriptor_sets(&desc_alloc_info).unwrap() };

    (descriptor_pool, descriptor_sets, desc_set_layouts)
}
