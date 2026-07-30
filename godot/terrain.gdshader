shader_type spatial;

// The terrain material.
//
// The mesh carries the surface kind in vertex COLOR as a one-hot channel --
// red grass, green forest, blue rock, and water as the absence of all three --
// rather than a finished tone. That is deliberate and unchanged: it keeps what
// the ground looks like out of Rust, so the art direction can change without
// recompiling the simulation's renderer.
//
// What changed is everything downstream of that. This used to select one of four
// flat colours and stop, which is why the ground read as poster paint and why
// every boundary between two kinds was a staircase following the 10 m vertex
// grid. Now the kinds are real photographed materials -- colour, normal and
// roughness -- blended with noise so the seams stop being straight, and lit
// rather than painted.
//
// # No triplanar, and that is a measurement rather than an oversight
//
// Triplanar projection exists to stop textures smearing on steep ground. This
// map has about 65 m of relief over 6 km, so the steepest slope anywhere is a
// few degrees and a plain world-XZ projection stretches by an amount nobody can
// see. Triplanar would have tripled the sample count to fix a problem the
// terrain does not have.

group_uniforms surfaces;
uniform sampler2D grass_colour : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D grass_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D grass_rough : hint_roughness_r, filter_linear_mipmap_anisotropic, repeat_enable;

uniform sampler2D forest_colour : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D forest_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D forest_rough : hint_roughness_r, filter_linear_mipmap_anisotropic, repeat_enable;

uniform sampler2D rock_colour : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D rock_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D rock_rough : hint_roughness_r, filter_linear_mipmap_anisotropic, repeat_enable;

uniform sampler2D shore_colour : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D shore_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D shore_rough : hint_roughness_r, filter_linear_mipmap_anisotropic, repeat_enable;

uniform sampler2D snow_colour : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D snow_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;

group_uniforms scale;
// Metres per texture repeat. A grass tile that reads correctly underfoot tiles
// visibly from a kilometre up, so everything is sampled twice -- once at this
// scale and once eight times larger -- and mixed. The large sample carries the
// far view and kills the corduroy; the small one carries the near view.
uniform float detail_metres = 5.0;
uniform float macro_ratio = 8.0;
// Metres over which the large-scale tint varies. Real ground is not one colour
// over six kilometres; without this the whole map reads as a single swatch
// however good the material on it is.
uniform float variation_metres = 420.0;
uniform float variation_strength : hint_range(0.0, 1.0) = 0.35;

group_uniforms blending;
// How far a boundary between two surface kinds wanders off the vertex grid, in
// metres. Zero puts every edge back on the 10 m lattice, which is exactly the
// staircase this exists to break.
uniform float blend_wander = 22.0;
uniform float blend_sharpness : hint_range(0.5, 8.0) = 2.2;

group_uniforms water_edge;
uniform vec3 water_colour : source_color = vec3(0.10, 0.17, 0.22);

group_uniforms weather;
// How much snow is lying, 0 to 1. Driven from the simulation's own ground model
// by `main.gd`, so a January frame is white because the republic is actually
// under snow rather than because somebody set a winter flag.
uniform float snow_amount : hint_range(0.0, 1.0) = 0.0;

group_uniforms survey;
// Faint banding every `contour_spacing` metres of height. Off by default now:
// contour lines are a map-drawing conceit and this ground is trying to look like
// ground. It survives for the survey overlay, where drawn-on lines are the point.
uniform float contour_strength : hint_range(0.0, 1.0) = 0.0;
uniform float contour_spacing = 8.0;

group_uniforms overlay;
// One channel of simulation state painted over the ground.
//
// Resources are invisible on the terrain by design, soil state is invisible
// because soil is, and track wear is invisible until it becomes a road. All
// three are things the simulation knows, so all three need somewhere to be
// seen. The traversal lattice is 100 m where the terrain is 10 m, so a 6 km map
// is a 60x60 image.
uniform sampler2D overlay_field : filter_linear, repeat_disable;
uniform float overlay_strength : hint_range(0.0, 1.0) = 0.0;
uniform vec3 overlay_low : source_color = vec3(0.75, 0.24, 0.18);
uniform vec3 overlay_high : source_color = vec3(0.30, 0.55, 0.35);
uniform float map_extent = 6000.0;

varying vec3 kind;
varying float height;
varying vec2 world_xz;


float hash21(vec2 p) {
	p = fract(p * vec2(233.34, 851.73));
	p += dot(p, p + 23.45);
	return fract(p.x * p.y);
}


float value_noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(hash21(i), hash21(i + vec2(1.0, 0.0)), u.x),
		mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), u.x),
		u.y
	);
}


float fbm3(vec2 p) {
	return value_noise(p) * 0.5 + value_noise(p * 2.03) * 0.3 + value_noise(p * 4.11) * 0.2;
}


// One material.
//
// **Only the albedo is sampled at two scales**, and that is a performance
// decision taken on a measurement rather than a guess. Sampling all three maps
// twice across five layers is thirty texture reads a fragment, which measured
// 13.3 ms p50 and 18.1 ms p95 -- past the 16.7 ms a 60 fps frame has, with the
// vegetation still to come.
//
// Albedo keeps the two-scale trick because visible tiling is a *colour*
// artefact: the eye picks out a repeating pattern of light and dark from a
// kilometre up. A repeating pattern in the normal or the roughness at that
// distance is below the noise floor of the lighting. So the expensive half buys
// nothing on two of the three maps.
void surface(
	sampler2D albedo_map, sampler2D normal_map, sampler2D rough_map,
	vec2 uv, out vec3 albedo, out vec3 normal, out float rough
) {
	// 0.35 rather than half: the near view is the one a player looks closely at,
	// so the detail sample keeps the larger share.
	albedo = mix(texture(albedo_map, uv).rgb, texture(albedo_map, uv / macro_ratio).rgb, 0.35);
	normal = texture(normal_map, uv).rgb;
	rough = texture(rough_map, uv).r;
}


void vertex() {
	kind = COLOR.rgb;
	height = VERTEX.y;
	world_xz = vec2(VERTEX.x, VERTEX.z);
}


void fragment() {
	vec2 uv = world_xz / detail_metres;

	// Weights start as the one-hot kind the mesh carries, then get pushed around
	// by noise. Without this every boundary is exactly where the vertex grid put
	// it, which at 10 m spacing over 6 km is a visible staircase -- the single
	// most artificial thing about the old ground.
	float wander = (fbm3(world_xz / blend_wander) - 0.5) * 2.0;
	vec3 weight = kind + wander * 0.55;

	// Water is the absence of the other three, so it gets what is left.
	float land = clamp(max(weight.r, max(weight.g, weight.b)), 0.0, 1.0);

	// Gravel in the band where land gives way to water.
	//
	// Keyed off how close this is to being water, NOT off absolute height. A
	// first pass used `height < shore_height`, which turned every low-lying part
	// of the map into beach -- including the whole forest, which happens to sit
	// low. Elevation is not distance from the water when the water is a river
	// running across a plain.
	float shore_w = smoothstep(0.02, 0.30, land) * (1.0 - smoothstep(0.30, 0.62, land));

	weight = clamp(weight, vec3(0.0), vec3(1.0));
	weight = pow(weight, vec3(blend_sharpness));

	float total = weight.r + weight.g + weight.b + shore_w + 0.0001;

	vec3 a_grass, n_grass, a_forest, n_forest, a_rock, n_rock, a_shore, n_shore;
	float r_grass, r_forest, r_rock, r_shore;
	surface(grass_colour, grass_normal, grass_rough, uv, a_grass, n_grass, r_grass);
	surface(forest_colour, forest_normal, forest_rough, uv, a_forest, n_forest, r_forest);
	surface(rock_colour, rock_normal, rock_rough, uv, a_rock, n_rock, r_rock);
	surface(shore_colour, shore_normal, shore_rough, uv, a_shore, n_shore, r_shore);

	// Bare earth is a *tint* on the grass rather than a fifth sampled layer.
	// Real meadow has scuffed patches, but a whole material set to express them
	// costs four texture reads a fragment everywhere on the map to be visible on
	// a few percent of it. Tinting towards the dirt colour is most of the read
	// for none of the samples.
	float scuff = smoothstep(0.55, 0.85, fbm3(world_xz / 95.0));
	a_grass = mix(a_grass, a_grass * vec3(1.35, 1.18, 0.92), scuff * 0.7);
	r_grass = mix(r_grass, 1.0, scuff * 0.4);

	// Forest floor sits under a canopy, so it is darker and greener than the bare
	// litter the photograph shows. Without this the woods read as desert from
	// above -- which is exactly how they looked before the trees existed, since
	// leaf litter in full sun is the colour of sand.
	a_forest *= vec3(0.62, 0.70, 0.55);

	vec3 albedo = (
		a_grass * weight.r + a_forest * weight.g + a_rock * weight.b
		+ a_shore * shore_w
	) / total;
	vec3 normal = (
		n_grass * weight.r + n_forest * weight.g + n_rock * weight.b
		+ n_shore * shore_w
	) / total;
	float rough = (
		r_grass * weight.r + r_forest * weight.g + r_rock * weight.b
		+ r_shore * shore_w
	) / total;

	// Under the waterline the ground is bed rather than land. Phase 5 gives the
	// water its own surface; this keeps the bed dark and wet-looking underneath.
	float submerged = 1.0 - land;
	albedo = mix(albedo, albedo * water_colour * 3.0, submerged);
	rough = mix(rough, 0.12, submerged);

	// Large-scale tint. Two octaves at very different scales so it reads as
	// geography -- a drier rise here, a damper hollow there -- rather than as a
	// pattern.
	float variation = fbm3(world_xz / variation_metres);
	albedo *= mix(1.0 - variation_strength, 1.0 + variation_strength * 0.45, variation);

	// Snow lies on the flat and blows off what little slope there is. NORMAL is
	// still the mesh normal here, which is what we want -- this is about the
	// ground's shape, not the texture's.
	if (snow_amount > 0.0) {
		float flatness = smoothstep(0.55, 0.92, NORMAL.y);
		// Hollows fill first, and the wind scours the noise-high ground.
		float drift = fbm3(world_xz / 60.0);
		float lying = clamp(snow_amount * 1.5 - drift * 0.5, 0.0, 1.0) * flatness;
		lying *= land;
		vec3 a_snow = mix(
			texture(snow_colour, uv).rgb, texture(snow_colour, uv / macro_ratio).rgb, 0.35
		);
		vec3 n_snow = mix(
			texture(snow_normal, uv).rgb, texture(snow_normal, uv / macro_ratio).rgb, 0.35
		);
		albedo = mix(albedo, a_snow, lying);
		normal = mix(normal, n_snow, lying);
		rough = mix(rough, 0.72, lying);
	}

	if (contour_strength > 0.0) {
		float band = fract(height / contour_spacing);
		float edge = fwidth(height / contour_spacing) * 1.5;
		float line = 1.0 - smoothstep(0.0, edge, min(band, 1.0 - band));
		albedo = mix(albedo, albedo * 0.82, line * contour_strength * 0.7);
	}

	if (overlay_strength > 0.0) {
		float v = texture(overlay_field, world_xz / map_extent).r;
		albedo = mix(albedo, mix(overlay_low, overlay_high, v), overlay_strength);
	}

	ALBEDO = albedo;
	NORMAL_MAP = normal;
	// Gentler than the map says. A normal map authored for a surface seen from
	// two metres is far too loud on ground seen from a kilometre, where it turns
	// into a shimmer that no amount of anisotropic filtering settles.
	NORMAL_MAP_DEPTH = 0.55;
	ROUGHNESS = clamp(rough, 0.05, 1.0);
	SPECULAR = 0.45;
}
