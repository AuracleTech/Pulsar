#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_ARB_shading_language_420pack : enable

layout (location = 0) in vec4 pos;
layout (location = 1) in vec2 uv;

layout (binding = 0) uniform UBO{
    mat4 transform;
} ubo;


layout (location = 0) out vec2 o_uv;
void main() {
    o_uv = uv;
    gl_Position = pos * ubo.transform;
}