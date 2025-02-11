#version 450

layout(location = 0) in vec3 norm;
//layout(location = 1) in flat uint mat_id;

layout(set = 0, binding = 0, std140) uniform Material {
    vec4 color;
    float noise;
    float data_0;
    float data_1;
    float data_2;
    mat4 padding;
} material;

layout(location = 0) out vec4 color_out;
layout(location = 1) out vec2 pos_out;
layout(location = 2) out vec2 norm_out;

void main() {
    color_out = material.color;
    pos_out = gl_FragCoord.xy;
    norm_out = norm.xy;
}
