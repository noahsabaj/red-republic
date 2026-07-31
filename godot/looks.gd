extends RefCounted

## The look, settled 2026-07-28.
##
## Three directions were rendered from the real scene -- the actual game, the
## actual republic, three hundred days in -- rather than mocked up, because a
## picture of a panel proves nothing about what 6 km of gently rolling ground
## looks like under a given light. Noah picked **survey**.
##
## The other two are not kept. An austere overcast north and a warm low evening
## were both defensible and neither was chosen, and a second copy of a decision
## is a thing that drifts. What they were and why they lost is in the commit
## that removed them.
##
## The brief was "visually beautiful, but not over the top", and survey answers
## it by being legible rather than dramatic: this game is half logistics, and
## the ground has to be readable before it is handsome.
##
## Everything here is presentation. Nothing in this file may change how a
## republic turns out, and nothing in it is read by the simulation.
##
## Deliberately no `class_name`: a global class is only registered by an editor
## import pass, so a freshly added one is invisible to `godot --path ...` until
## somebody remembers to re-import. That failed silently once already. Callers
## `preload` this file instead, which needs no registry.

class Look:
	var name: String
	var sun_colour: Color
	var sun_energy: float
	## Degrees above the horizon. Low sun means long shadows and raking light.
	var sun_elevation: float
	var sun_azimuth: float
	var ambient_colour: Color
	var ambient_energy: float
	var sky_top: Color
	var sky_horizon: Color
	var ground_horizon: Color
	var fog_colour: Color
	## Per metre. Godot extinction is exp(-density * depth), so on a 6 km map
	## the useful range is tiny: 0.00035 -- which felt like a small number --
	## put 65% fog at three kilometres and turned the first render into soup.
	## These give roughly 5-15% at 3 km, which is depth rather than weather.
	var fog_density: float
	var grass: Color
	var forest: Color
	var rock: Color
	var water: Color
	var building: Color
	var vehicle: Color
	var road_dirt: Color
	var road_paved: Color
	## The two blocs on the frontier. Red for the Eastern Bloc and blue for the
	## Western Alliance, which is the convention the reference map uses and the
	## only part of this palette that is not free to change.
	var bloc_east: Color
	var bloc_west: Color
	## Material families for the building kit, keyed by `building_art.Tone`.
	var tones: Dictionary
	var tonemap_exposure: float
	var contour_strength: float
	## Colour grading, applied after the tonemapper. Small numbers: this is the
	## last 5% of a look, and reaching for it to fix flat lighting is how you get
	## a crushed, over-saturated image that still has no form in it.
	var grade_brightness: float
	var grade_contrast: float
	var grade_saturation: float
	## Share of the sky under cloud. Presentation only — this is emphatically not
	## the weather the simulation models, which is a temperature and a
	## precipitation figure per day and is read by heating demand and the ground,
	## never by anything in here.
	var cloud_cover: float


## C. Survey — the map Moscow drew, stood up in three dimensions.
##
## Cool, crisp, high clarity, with faint contour banding across the relief. It
## leans on the fiction the founding screen already tells: the posting was
## surveyed before it was assigned. It is also the most legible of the three for
## a game that is half logistics, and the most likely to feel cold.
## The one look. Named for what it is rather than numbered.
static func current() -> Look:
	var l := Look.new()
	l.name = "survey"
	l.bloc_east = Color(0.72, 0.20, 0.18)
	l.bloc_west = Color(0.22, 0.42, 0.72)
	# Render, brick, concrete, metal, timber -- the five families the kit works
	# in. Not per-building colours: a hundred buildings each with its own tint
	# is noise, and things that are alike should look alike.
	l.tones = {
		0: Color(0.78, 0.76, 0.70),
		1: Color(0.60, 0.42, 0.34),
		2: Color(0.68, 0.68, 0.66),
		3: Color(0.55, 0.57, 0.60),
		4: Color(0.56, 0.46, 0.34),
	}
	l.road_dirt = Color(0.40, 0.35, 0.29)
	l.road_paved = Color(0.34, 0.35, 0.37)
	# A warm mid-morning sun rather than the near-overhead 62 degrees this used
	# to sit at. That number was chosen against a renderer that drew no shadows,
	# so nothing was lost by putting the sun where shadows would be shortest --
	# and everything about how ground and buildings read depends on it now.
	# Around 30 degrees is where a 15 m building lays down a shadow about 26 m
	# long, which is what tells you it is a building and not a painted rectangle.
	l.sun_colour = Color(1.0, 0.96, 0.90)
	l.sun_energy = 1.6
	l.sun_elevation = 30.0
	l.sun_azimuth = 138.0
	# Ambient now comes off the sky, so this is only the multiplier on it. The
	# colour below is kept because the compatibility renderer has no sky ambient
	# and falls back to it.
	l.ambient_colour = Color(0.60, 0.66, 0.72)
	l.ambient_energy = 0.35
	# A real zenith is much deeper than this used to be. The old pair was two
	# steps of the same pale blue, which is what a two-colour gradient sky needs
	# to avoid banding and what a scattering sky does not.
	l.sky_top = Color(0.11, 0.28, 0.58)
	l.sky_horizon = Color(0.66, 0.76, 0.84)
	l.ground_horizon = Color(0.40, 0.44, 0.45)
	l.fog_colour = Color(0.74, 0.80, 0.86)
	# Per metre, and the old 0.000011 gave 3.2% at three kilometres while the
	# comment above claimed 5-15%. That is not depth, it is nothing, and it is
	# why the far side of the map read as hard as the near side.
	#
	# 0.00009 was the first correction and overshot: 24% at 3 km is a genuinely
	# hazy day, which was invisible on flat-colour ground and drowned the far
	# half of the map once the ground had detail worth seeing. 0.00005 is about
	# 14% at 3 km — depth, with the distance still legible.
	l.fog_density = 0.00005
	l.grass = Color(0.42, 0.48, 0.35)
	l.forest = Color(0.24, 0.34, 0.26)
	l.rock = Color(0.55, 0.55, 0.53)
	l.water = Color(0.30, 0.44, 0.52)
	l.building = Color(0.80, 0.78, 0.72)
	l.vehicle = Color(0.48, 0.17, 0.15)
	l.tonemap_exposure = 1.0
	l.contour_strength = 1.0
	l.grade_brightness = 1.0
	l.grade_contrast = 1.06
	l.grade_saturation = 1.05
	# Enough to break the sky up and cast some variety of light, not enough to
	# make every screenshot overcast.
	l.cloud_cover = 0.45
	return l
