shader_type sky;

// The sky.
//
// A custom sky shader rather than `PhysicalSkyMaterial` or a photographed HDRI,
// and the reason is the sun. A panorama gives the most convincing clouds for the
// least work, but it bakes the sun into the image: the light direction becomes a
// thing you match to a photograph rather than a thing you set, and a day/night
// cycle -- which this game wants more than most, because one real second is one
// in-game second -- would need a different photograph every few minutes.
// `PhysicalSkyMaterial` keeps the sun and has no clouds at all.
//
// So: scattering that follows LIGHT0, with cloud cover on top of it. The sun can
// go anywhere and the sky follows.
//
// Everything here is presentation. Nothing in this file may change how a
// republic turns out.

// Rayleigh scattering is what makes a clear sky blue and a low sun red: short
// wavelengths scatter far more, so with the sun overhead the blue arrives from
// every direction, and at a shallow angle the blue has been scattered out of the
// path before it reaches you and the red is what is left.
uniform vec3 rayleigh_tint : source_color = vec3(0.19, 0.45, 0.90);
uniform float rayleigh_strength : hint_range(0.0, 4.0) = 1.6;

// Mie scattering is the haze around the sun -- larger particles, scattering all
// wavelengths roughly equally and strongly forwards, which is why the sky is
// bright and white close to the sun and not just blue everywhere.
uniform vec3 mie_tint : source_color = vec3(1.0, 0.96, 0.90);
uniform float mie_strength : hint_range(0.0, 2.0) = 0.28;
uniform float mie_forward : hint_range(0.0, 0.99) = 0.76;

uniform vec3 zenith_colour : source_color = vec3(0.16, 0.34, 0.62);
uniform vec3 horizon_colour : source_color = vec3(0.72, 0.80, 0.88);
uniform vec3 ground_colour : source_color = vec3(0.26, 0.27, 0.26);

uniform float sun_disk_size : hint_range(0.0, 0.2) = 0.017;

// Cloud cover, as a share of the sky. 0 is clear, 1 is overcast.
uniform float cloud_cover : hint_range(0.0, 1.0) = 0.42;
uniform float cloud_sharpness : hint_range(0.5, 8.0) = 2.6;
uniform vec3 cloud_lit : source_color = vec3(1.0, 0.98, 0.95);
uniform vec3 cloud_shade : source_color = vec3(0.70, 0.73, 0.79);
// Cells of cloud across the visible sky. Small numbers do not mean "big soft
// clouds", they mean the whole sky lands inside one noise cell and comes back
// flat: at 2.4 the entire band a 14-degree camera can see spanned about one unit
// of noise, which is less than a single octave.
uniform float cloud_scale = 16.0;
uniform float cloud_drift = 0.004;
uniform float cloud_altitude = 0.12;

// How dark it is, 0 broad daylight to 1 the middle of the night — the same
// global every lit window reads.
//
// **The sky has to be told, and cannot work it out.** `LIGHT0_DIRECTION` is the
// only thing it has, and at night `main.gd` deliberately points that at a moon:
// a DirectionalLight below the horizon lights the underside of the world and
// nothing else, so the light is mirrored back up, made cold and made faint.
// That is the right answer for the ground and it leaves the sky computing a
// scattering solution for a sun that is very much up — a night frame came back
// with a dark republic under a bright blue afternoon.
global uniform float night;

// A night sky, mixed toward rather than computed. Scattering at night is
// starlight and airglow, which this model has no term for and which is not
// worth a term: what it has to do is be dark, be blue, and keep the clouds
// readable as shapes against it.
uniform vec3 night_zenith : source_color = vec3(0.012, 0.020, 0.045);
uniform vec3 night_horizon : source_color = vec3(0.045, 0.058, 0.090);


// Value noise. Deliberately not a texture: the sky is the one surface with no
// scale reference, so a tiling artefact in it is visible from anywhere on the
// map and there is nothing to hide it behind.
float hash(vec2 p) {
	p = fract(p * vec2(233.34, 851.73));
	p += dot(p, p + 23.45);
	return fract(p.x * p.y);
}


float value_noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	// Smoothstep the interpolant, or the cell boundaries show as a grid.
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
		u.y
	);
}


// Five octaves. Cloud edges are the thing the eye reads, and three octaves
// leaves them smooth enough to look like fog rather than cloud.
float fbm(vec2 p) {
	float total = 0.0;
	float amplitude = 0.5;
	for (int i = 0; i < 5; i++) {
		total += value_noise(p) * amplitude;
		p *= 2.02;
		amplitude *= 0.5;
	}
	return total;
}


void sky() {
	float up = EYEDIR.y;
	float sun_dot = dot(EYEDIR, LIGHT0_DIRECTION);

	// Below the horizon is ground, not sky. Softened over a few degrees so the
	// horizon is a horizon rather than a cut.
	float sky_mask = smoothstep(-0.04, 0.02, up);

	// The base gradient. `pow` on the height rather than a straight mix, because
	// the real thing changes fastest near the horizon.
	float height = pow(clamp(up, 0.0, 1.0), 0.42);
	vec3 base = mix(horizon_colour, zenith_colour, height);

	// Rayleigh: more scattering along a long path through the atmosphere, which
	// is what a shallow view angle is.
	//
	// Capped, and the cap is the whole difference between a sky and a white
	// wash. The physical term runs to infinity at the horizon; left uncapped it
	// added enough brightness at low angles to bleach out the entire band a
	// camera at 15 degrees can see, which is most of the sky this game shows.
	float air_mass = min(1.0 / max(up + 0.10, 0.10), 5.0);
	vec3 rayleigh = rayleigh_tint * rayleigh_strength * (0.030 * air_mass);

	// Henyey-Greenstein, the standard cheap forward-scattering phase function.
	float g = mie_forward;
	float denom = 1.0 + g * g - 2.0 * g * sun_dot;
	float phase = (1.0 - g * g) / (4.0 * 3.14159265 * pow(max(denom, 0.0001), 1.5));
	vec3 mie = mie_tint * mie_strength * phase * air_mass;

	vec3 colour = base + rayleigh * 0.35 + mie;

	// The sun itself. `sun_disk_size` is the angular radius as a dot-product
	// threshold; the real sun is about half a degree across and looking straight
	// at it should be blinding rather than merely bright.
	float disk = smoothstep(1.0 - sun_disk_size, 1.0 - sun_disk_size * 0.35, sun_dot);
	colour += LIGHT0_COLOR * LIGHT0_ENERGY * disk * 12.0;

	// Clouds, on a notional flat layer at `cloud_altitude`. Projecting the view
	// direction onto that plane is what gives the perspective foreshortening
	// that makes cover read as a ceiling rather than as a pattern painted on a
	// dome -- clouds bunch towards the horizon exactly as they should.
	if (up > 0.0) {
		vec2 plane = EYEDIR.xz / max(up, 0.001) * cloud_altitude;
		vec2 uv = plane * cloud_scale + vec2(TIME * cloud_drift, TIME * cloud_drift * 0.6);

		float density = fbm(uv);
		// A second, slower layer at a different scale, so the cover has depth
		// rather than being one sheet of noise.
		density = mix(density, fbm(uv * 0.43 + 17.0), 0.45);

		// Normalise before thresholding, and this is the part that had to be
		// measured rather than assumed. Five octaves at halving amplitude do NOT
		// fill 0 to 1: they sum to about 0.97 with a mean near 0.48 and a spread
		// of roughly a tenth either side. Two passes of this shader produced a
		// completely cloudless sky because the threshold and the smoothstep width
		// were both written for a 0-to-1 variable -- 45% cover put the cut at
		// 0.55, above nearly every value the noise returns, and the blend band
		// was three times wider than the noise's whole range.
		//
		// Stretching the useful band to 0..1 first means `cloud_cover` is a
		// share of sky again, and the sharpness is a real edge.
		density = clamp((density - 0.32) / 0.34, 0.0, 1.0);

		float amount = smoothstep(
			1.0 - cloud_cover, 1.0 - cloud_cover + (0.55 / cloud_sharpness), density
		);

		// Fade the layer out at the horizon itself, where a flat cloud plane
		// stretches to infinity and would paint the sky solid.
		//
		// The band has to be narrow. A first pass faded over 0 to 0.22, which is
		// above the top of the frame when the camera sits at the 15 degrees this
		// game is most often watched from -- so the clouds were correct, present,
		// and invisible in every shot taken to judge them.
		amount *= smoothstep(0.010, 0.075, up);

		// Light the cloud by how much noise sits between this point and the sun.
		// Not a real scattering solve -- a cheap gradient towards the sun, which
		// is enough to give the tops rims and the undersides weight.
		float towards_sun = fbm(uv + LIGHT0_DIRECTION.xz * 0.35);
		float lit = clamp((density - towards_sun) * 2.5 + 0.55, 0.0, 1.0);
		vec3 cloud = mix(cloud_shade, cloud_lit, lit);
		// Clouds near the sun catch its colour.
		cloud += LIGHT0_COLOR.rgb * pow(max(sun_dot, 0.0), 8.0) * 0.35;

		colour = mix(colour, cloud, amount * sky_mask);
	}

	// Down to night, last of all, so the clouds and the horizon gradient go with
	// it rather than being computed in daylight and dimmed afterwards.
	vec3 dark = mix(night_horizon, night_zenith, clamp(EYEDIR.y, 0.0, 1.0));
	// Clouds keep a little of their shape against the night sky rather than
	// disappearing into it, which is what a real overcast night looks like.
	colour = mix(colour, mix(dark, colour * 0.10 + dark, 0.55), night);

	COLOR = mix(ground_colour * mix(1.0, 0.12, night), colour, sky_mask);
}
