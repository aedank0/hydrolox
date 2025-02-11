#version 450

layout(input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput color_in;
layout(input_attachment_index = 1, set = 0, binding = 1) uniform subpassInput norm_in;
layout(input_attachment_index = 2, set = 0, binding = 2) uniform subpassInput pos_in;
layout(input_attachment_index = 3, set = 0, binding = 3) uniform subpassInput depth_in;

layout(location = 0) out vec4 color_out;

void main() {
    vec3 pos = vec3(subpassLoad(pos_in).xy, 1.0 - subpassLoad(depth_in).r);

    vec2 norm_xy = subpassLoad(norm_in).xy;
    vec3 norm = vec3(norm_xy, sqrt(1.0 - (norm_xy.x * norm_xy.x + norm_xy.y * norm_xy.y)));

    vec3 sun_angle = normalize(vec3(1.0, 0.0, 0.0));
    vec3 dark_color = vec3(0.01, 0.01, 0.01);

    float light = dot(norm, sun_angle);
    if (pos.z == 1.0) {
        light = 1.0;
    }

    color_out = vec4(subpassLoad(color_in).rgb * light + dark_color * (1.0 - light), 1.0);
}
