struct UniformData {
    @size(16) image_extent: vec2<f32>,
}

@group(0) @binding(0)
var display_texture: texture_2d<f32>;
@group(0) @binding(1)
var display_sampler: sampler;
@group(0) @binding(2)
var<uniform> uniform_data: UniformData;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32(vertex_index & 1);
    let y = f32(vertex_index >> 1);
    let pos = vec2(x, y);

    var out = VertexOutput();
    out.position = vec4(pos * 4.0 - 1.0, 0.0, 1.0);
    out.uv = pos * 2.0;
    out.uv.y = 1.0 - out.uv.y;
    out.uv = out.uv * uniform_data.image_extent;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(display_texture, display_sampler, in.uv);
    return color * (255.0 / 31.0);
}
