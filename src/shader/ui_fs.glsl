#version 450

layout(set = 1, binding = 0) uniform sampler2D tex;

layout(location = 0) in vec2 uv;
layout(location = 1) in vec4 color;

layout(location = 0) out vec4 out_color;

void main() {
    vec4 srgb = texture(tex, uv) * color;
    bvec3 cutoff = lessThan(srgb.rgb, vec3(0.0031308));
    vec3 lower = srgb.rgb * vec3(12.92);
    vec3 higher = vec3(1.055) * pow(srgb.rgb, vec3(1.0 / 2.4)) - vec3(0.055);
    out_color = vec4(mix(higher, lower, vec3(cutoff)), srgb.a);
    //out_color = srgb;
}
