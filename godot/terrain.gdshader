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

varying vec3 kind;

void vertex() {
	kind = COLOR.rgb;
}

void fragment() {
	// One-hot, so this is a selection rather than a blend. Water is the absence
	// of the other three, which is why it is the base the others are mixed over.
	vec3 albedo = water_colour;
	albedo = mix(albedo, grass_colour, kind.r);
	albedo = mix(albedo, forest_colour, kind.g);
	albedo = mix(albedo, rock_colour, kind.b);

	ALBEDO = albedo;
	// Water reads as water because it is smooth, not because it is blue.
	ROUGHNESS = mix(0.25, 0.95, max(kind.r, max(kind.g, kind.b)));
	SPECULAR = mix(0.6, 0.15, max(kind.r, max(kind.g, kind.b)));
}
