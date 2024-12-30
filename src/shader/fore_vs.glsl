#version 450

layout(location = 0) in vec3 pos;
layout(location = 1) in vec3 norm;
layout(location = 2) in uvec4 data;

struct Motor {
    vec4 v;
    vec4 m;
};

layout(push_constant, std430) uniform PushData {
    mat4 proj;
    Motor obj;
    Motor cam;
} push_data;

layout(location = 0) out vec3 norm_out;
layout(location = 1) out flat uint mat_id_out;

vec4 transform_point(in Motor q, in vec4 p) {
    vec3 a = cross(q.v.xyz, p.xyz) + (q.m.xyz * p.w);

    return vec4(
        p.xyz + 2.0 * (a * q.v.w + cross(q.v.xyz, a) - (q.v.xyz * q.m.w * p.w)),
        p.w
    );
}

void main() {
    gl_Position = push_data.proj * transform_point(push_data.cam, transform_point(push_data.obj, vec4(pos, 1.0)));
    norm_out = norm;
    mat_id_out = data.x;
}
