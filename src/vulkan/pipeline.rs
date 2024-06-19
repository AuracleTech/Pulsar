use super::{device::AAADevice, surface::AAASurface};
use crate::{model::Vertex, shaders::Shader};
use ash::vk;
use std::mem;

pub fn create_pipeline(
    device: &AAADevice,
    surface: &AAASurface,
    renderpass: vk::RenderPass,
    desc_set_layouts: [vk::DescriptorSetLayout; 1],
) -> (
    vk::Pipeline,
    [vk::Viewport; 1],
    [vk::Rect2D; 1],
    Vec<vk::Pipeline>,
    vk::PipelineLayout,
    vk::ShaderModule,
    vk::ShaderModule,
) {
    let vertex_shader = Shader::from_filename("vert", vk::ShaderStageFlags::VERTEX, device);
    let frag_shader = Shader::from_filename("frag", vk::ShaderStageFlags::FRAGMENT, device);

    let shader_stage_create_infos = [
        vertex_shader.pipeline_shader_stage_create_info,
        frag_shader.pipeline_shader_stage_create_info,
    ];

    let layout_create_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&desc_set_layouts);
    let pipeline_layout = unsafe {
        device
            .ash
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap()
    };

    let vertex_input_binding_descriptions = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: mem::size_of::<Vertex>() as u32,
        input_rate: vk::VertexInputRate::VERTEX,
    }];
    let vertex_input_attribute_descriptions = [
        vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: mem::offset_of!(Vertex, pos) as u32,
        },
        vk::VertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: vk::Format::R32G32_SFLOAT,
            offset: mem::offset_of!(Vertex, uv) as u32,
        },
        vk::VertexInputAttributeDescription {
            location: 2,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: mem::offset_of!(Vertex, color) as u32,
        },
    ];

    let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_attribute_descriptions(&vertex_input_attribute_descriptions)
        .vertex_binding_descriptions(&vertex_input_binding_descriptions);
    let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::TRIANGLE_LIST,
        ..Default::default()
    };

    let viewports = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: surface.capabilities.current_extent.width as f32,
        height: surface.capabilities.current_extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissors = [surface.capabilities.current_extent.into()];
    let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
        .scissors(&scissors)
        .viewports(&viewports);

    let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
        front_face: vk::FrontFace::COUNTER_CLOCKWISE,
        line_width: 1.0,
        polygon_mode: vk::PolygonMode::FILL,
        ..Default::default()
    };
    let multisample_state_info = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let noop_stencil_state = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::KEEP,
        depth_fail_op: vk::StencilOp::KEEP,
        compare_op: vk::CompareOp::ALWAYS,
        ..Default::default()
    };
    let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: 1,
        depth_write_enable: 1,
        depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
        front: noop_stencil_state,
        back: noop_stencil_state,
        max_depth_bounds: 1.0,
        ..Default::default()
    };
    let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
        blend_enable: 0,
        src_color_blend_factor: vk::BlendFactor::SRC_COLOR,
        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_COLOR,
        color_blend_op: vk::BlendOp::ADD,
        src_alpha_blend_factor: vk::BlendFactor::ZERO,
        dst_alpha_blend_factor: vk::BlendFactor::ZERO,
        alpha_blend_op: vk::BlendOp::ADD,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    }];
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op(vk::LogicOp::CLEAR)
        .attachments(&color_blend_attachment_states);

    let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state_info =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_state);

    let graphic_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stage_create_infos)
        .vertex_input_state(&vertex_input_state_info)
        .input_assembly_state(&vertex_input_assembly_state_info)
        .viewport_state(&viewport_state_info)
        .rasterization_state(&rasterization_info)
        .multisample_state(&multisample_state_info)
        .depth_stencil_state(&depth_state_info)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state_info)
        .layout(pipeline_layout)
        .render_pass(renderpass);

    let graphics_pipelines = unsafe {
        device
            .ash
            .create_graphics_pipelines(vk::PipelineCache::null(), &[graphic_pipeline_info], None)
            .expect("Unable to create graphics pipeline")
    };

    let graphic_pipeline = graphics_pipelines[0];

    (
        graphic_pipeline,
        viewports,
        scissors,
        graphics_pipelines,
        pipeline_layout,
        vertex_shader.module,
        frag_shader.module,
    )
}
