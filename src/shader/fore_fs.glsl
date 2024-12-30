#version 450

layout(location = 0) in vec3 norm;
layout(location = 1) in flat uint mat_id;

struct MatData {
    vec4 color;
    float noise;
    float data[3];
};

layout(set = 0, binding = 0, std140) uniform Materials {
    MatData mats[1024];
} materials;

layout(location = 0) out vec4 color_out;
layout(location = 1) out vec2 pos_out;
layout(location = 2) out vec2 norm_out;

void main() {
    color_out = materials.mats[mat_id].color;
    pos_out = gl_FragCoord.xy;
    norm_out = norm.xy;
}
