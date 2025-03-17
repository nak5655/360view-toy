const PI = 3.1415926;
const TAU = 1.5707963;

struct Uniforms {
    // 視野
    aov: f32, // 視点
    look_at: vec3<f32>, // 視点上方向(単位ベクトル)
    up: vec3<f32>, // 視点右方向(単位ベクトル)
    right: vec3<f32>
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var texture: texture_2d<f32>;
@group(0) @binding(2) var texture_sampler: sampler;

struct VertexIn {
    @builtin(vertex_index) vertex_index: u32,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>,
}

@vertex fn vs_main(in: VertexIn) -> VertexOut {
    // 描画領域を覆う矩形の頂点
    let uv = vec2f(vec2u((in.vertex_index << 1) & 2, in.vertex_index & 2));
    let position = vec4f(uv * 2. - 1., 0., 1.);
    return VertexOut(position, uv);
}

@fragment fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // 視点(look_at)を基準とした場合の描画ピクセルの相対位置
    let yaw = uniforms.aov * (in.uv.x - 0.5);
    let pitch = uniforms.aov * (in.uv.y - 0.5);

    // ヨー, ピッチから球面座標に変換
    let sphereCoord = normalize(uniforms.look_at + yaw * uniforms.right + pitch * uniforms.up);

    // 球面座標からequirectangularの平面座標に変換
    let x = atan2(sphereCoord.z, sphereCoord.x);
    let y = atan2(sphereCoord.y, sqrt(sphereCoord.x * sphereCoord.x + sphereCoord.z * sphereCoord.z));

    // 平面座標からテクスチャの色を取得
    let color = textureSample(texture, texture_sampler, vec2(x / (2 * PI) + 0.5, 0.5 - y / PI));
    return color;
}