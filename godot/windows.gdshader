shader_type spatial;
render_mode cull_back, diffuse_burley, specular_schlick_ggx;

// Lit windows, on their own surface so nothing has to mask them.
//
// **A global uniform rather than a parameter per material.** A republic has a
// hundred and thirty building kinds and therefore a hundred and thirty of these
// materials; writing a night factor into each of them every frame would be a
// hundred and thirty writes to say one thing. Godot's global shader parameters
// are exactly the mechanism for a value the whole scene shares, and the sun is
// as shared as a value gets.
global uniform float night;

uniform vec3 glass_colour : source_color = vec3(0.20, 0.22, 0.25);
uniform vec3 lamp_colour : source_color = vec3(1.00, 0.83, 0.52);

void fragment() {
	ALBEDO = glass_colour;
	// Glass is smooth and reflective by day, which is most of what makes a
	// window band read as glazing rather than as a painted stripe.
	ROUGHNESS = 0.14;
	METALLIC = 0.05;
	SPECULAR = 0.6;
	// And warm by night. Squared so the change happens over dusk rather than
	// linearly across the whole afternoon — a lamp goes on when it gets dark,
	// not gradually from lunchtime.
	EMISSION = lamp_colour * night * night * 2.4;
}
