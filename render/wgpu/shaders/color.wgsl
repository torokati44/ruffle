/// Shader used for drawing solid color fills.

#import common

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) barycentric: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) barycentric: vec4<f32>,
};

#if use_push_constants == true
    var<push_constant> pc: common::PushConstants;
#else
    @group(1) @binding(0) var<uniform> transforms: common::Transforms;
    @group(2) @binding(0) var<uniform> colorTransforms: common::ColorTransforms;
#endif

@vertex
fn main_vertex(in: VertexInput) -> VertexOutput {
    #if use_push_constants == true
        var transforms = pc.transforms;
        var colorTransforms = pc.colorTransforms;
    #endif
    let pos = common::globals.view_matrix * transforms.world_matrix * vec4<f32>(in.position.x, in.position.y, 0.0, 1.0);
    let color = saturate(in.color * colorTransforms.mult_color + colorTransforms.add_color);
    return VertexOutput(pos, vec4<f32>(color.rgb * color.a, color.a), in.barycentric);
}

@fragment
fn main_fragment(in: VertexOutput) -> @location(0) vec4<f32> {

  let d = fwidth(in.barycentric);
  let f = step(d, in.barycentric);
  let edgeFactor = min(min(f.x, f.y), f.z);
  //return in.color;
  return vec4<f32>(min(vec3<f32>(1.0-edgeFactor), in.color.rgb), 1.0-edgeFactor);
/*
  if(in.barycentric.x < 0.05 || in.barycentric.y < 0.05 || in.barycentric.z < 0.05) {
    return in.color;
  } else {
    return vec4<f32>(0.0);
  }*/

}
