shader_type spatial;

// The terrain material.
//
// The mesh carries the surface kind in vertex COLOR as a one-hot channel --
// red grass, green forest, blue rock, black water -- rather than a finished
// tone. That is deliberate: it keeps what the ground looks like out of Rust, so
// the art direction can change without recompiling the simulation's renderer.
//
// These tones are a working placeholder, not the chosen look. The map has about
// 65 m of relief over 6 km, which is gentle enough that a fully saturated
// albedo hides it completely -- the first render of this scene looked
// perfectly flat and the ground was not. Muted tones let the lighting do its
// job, which is also what makes the shading a usable check that the mesh
// normals are right.

uniform vec3 grass_colour : source_color = vec3(0.35, 0.42, 0.24);
uniform vec3 forest_colour : source_color = vec3(0.20, 0.29, 0.17);
uniform vec3 rock_colour : source_color = vec3(0.44, 0.42, 0.39);
uniform vec3 water_colour : source_color = vec3(0.19, 0.28, 0.35);

// Faint banding every `contour_spacing` metres of height, for the survey look.
// Zero strength removes it entirely, which is how the other two looks turn it
// off without a second shader.
uniform float contour_strength : hint_range(0.0, 1.0) = 0.0;
uniform float contour_spacing = 8.0;

// The overlay: one channel of simulation state painted over the ground.
//
// Resources are invisible on the terrain by design, soil state is invisible
// because soil is, and track wear is invisible until it becomes a road. All
// three are things the simulation knows, so all three need somewhere to be
// seen. A sampled texture is the cheap way: the traversal lattice is 100 m
// where the terrain is 10 m, so a 6 km map is a 60x60 image.
uniform sampler2D overlay_field : filter_linear, repeat_disable;
uniform float overlay_strength : hint_range(0.0, 1.0) = 0.0;
uniform vec3 overlay_low : source_color = vec3(0.75, 0.24, 0.18);
uniform vec3 overlay_high : source_color = vec3(0.30, 0.55, 0.35);
uniform float map_extent = 6000.0;

varying vec3 kind;
varying float height;
varying vec2 ground_uv;

void vertex() {
	kind = COLOR.rgb;
	height = VERTEX.y;
	// Position on the map rather than mesh UV, so the overlay lines up with the
	// lattice whatever resolution the terrain was generated at.
	ground_uv = vec2(VERTEX.x, VERTEX.z) / map_extent;
}

void fragment() {
	// One-hot, so this is a selection rather than a blend. Water is the absence
	// of the other three, which is why it is the base the others are mixed over.
	vec3 albedo = water_colour;
	albedo = mix(albedo, grass_colour, kind.r);
	albedo = mix(albedo, forest_colour, kind.g);
	albedo = mix(albedo, rock_colour, kind.b);

	// A contour line is a band near the top of each height interval. fwidth
	// keeps it roughly one pixel wide however far away the ground is, so it
	// does not turn into moire on the far side of a 6 km map.
	if (contour_strength > 0.0) {
		float band = fract(height / contour_spacing);
		float edge = fwidth(height / contour_spacing) * 1.5;
		float line = 1.0 - smoothstep(0.0, edge, min(band, 1.0 - band));
		albedo = mix(albedo, albedo * 0.82, line * contour_strength * 0.7);
	}

	if (overlay_strength > 0.0) {
		float v = texture(overlay_field, ground_uv).r;
		albedo = mix(albedo, mix(overlay_low, overlay_high, v), overlay_strength);
	}

	ALBEDO = albedo;
	// Water reads as water because it is smooth, not because it is blue.
	ROUGHNESS = mix(0.25, 0.95, max(kind.r, max(kind.g, kind.b)));
	SPECULAR = mix(0.6, 0.15, max(kind.r, max(kind.g, kind.b)));
}
