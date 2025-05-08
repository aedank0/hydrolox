#version 450

layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 uv;
layout(location = 2) in vec4 color;

layout(set = 0, binding = 0) uniform UIData {
    vec2 res;
} data;

layout(location = 0) out vec2 out_uv;
layout(location = 1) out vec4 out_color;

void main() {
    gl_Position = vec4(pos / data.res * 2.0 - 1.0, 0.0, 1.0);
    out_uv = uv;
    out_color = color;
}
