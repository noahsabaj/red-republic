extends RefCounted

## What each building looks like, authored per kind.
##
## This is art data and it lives here rather than on `BuildingDef`, because
## height, roof type and whether there is a chimney change nothing about how a
## republic runs. A field no system reads has no business being simulation
## state, and keeping it here means the look can be changed without recompiling
## Rust.
##
## The simulation still owns everything physical: real width and depth in metres
## come from `BuildingDef` and are never guessed at here. Those footprints are
## large -- tens of metres -- so a building needs real height or it reads as a
## slab painted on the ground. The first pass at this table was too timid and
## only a close render said so. What this adds is the
## third dimension and the character, which the simulation has no opinion about.
##
## **Every kind must have a row.** `check()` fails loudly on an unauthored one
## rather than letting it fall back to a default, because a defaulted building is
## a decision nobody made — the same rule that puts `heat: 0.0` explicitly on a
## sawmill in the Rust table.

enum Roof { FLAT, PITCH, SAWTOOTH }

## Broad material families. Not per-building colours: a roster of a hundred
## buildings each with its own tint is noise, and the point of a kit is that
## things which are alike look alike.
enum Tone { RENDER, BRICK, CONCRETE, METAL, TIMBER }


class Art:
	var storeys: int
	var storey_m: float
	var roof: int
	var stacks: int
	var silos: int
	var gantry: bool
	var tone: int

	func _init(s: int, sm: float, r: int, st: int, si: int, g: bool, t: int) -> void:
		storeys = s
		storey_m = sm
		roof = r
		stacks = st
		silos = si
		gantry = g
		tone = t

	func height() -> float:
		return float(storeys) * storey_m


## Keyed by the name `BuildingDef` authors, which is what the shell exposes.
static func table() -> Dictionary:
	var F := Roof.FLAT
	var P := Roof.PITCH
	var S := Roof.SAWTOOTH
	return {
		# Housing. A five-storey walk-up is the Khrushchevka; the small house is
		# a single storey with a pitched roof.
		"Small House": Art.new(1, 3.2, P, 0, 0, false, Tone.TIMBER),
		"Apartment Block": Art.new(5, 3.0, F, 0, 0, false, Tone.RENDER),
		# The panel block is the one that needs prefab panels to exist, so it is
		# taller than the walk-up and concrete rather than rendered -- the point of
		# it is that you can see it was assembled from parts.
		"Panel Block": Art.new(9, 3.0, F, 0, 0, false, Tone.CONCRETE),

		# Extraction. The gantry is what makes a mine read as a mine from above.
		"Woodcutter Post": Art.new(1, 4.0, P, 0, 0, false, Tone.TIMBER),
		"Gravel Quarry": Art.new(1, 5.0, F, 0, 2, false, Tone.CONCRETE),
		"Coal Mine": Art.new(3, 5.0, F, 0, 0, true, Tone.METAL),
		"Iron Ore Mine": Art.new(3, 5.0, F, 0, 0, true, Tone.METAL),
		"Oil Pump": Art.new(1, 3.0, F, 0, 1, false, Tone.METAL),

		# Heavy industry. Sawtooth roofs and chimneys.
		"Sawmill": Art.new(1, 8.5, S, 0, 0, false, Tone.TIMBER),
		"Brickworks": Art.new(2, 5.0, S, 1, 0, false, Tone.BRICK),
		"Steel Mill": Art.new(3, 6.5, S, 2, 0, false, Tone.METAL),
		"Oil Refinery": Art.new(2, 6.0, F, 1, 3, false, Tone.METAL),
		"Machine Works": Art.new(2, 6.0, S, 1, 0, false, Tone.CONCRETE),

		# The construction-materials chain. All four are silos and gantries rather
		# than sheds, because that is what these actually look like: a cement works
		# is a stack and a row of silos, and a precast yard is a crane over a slab.
		"Cement Works": Art.new(3, 7.0, F, 1, 3, false, Tone.CONCRETE),
		"Concrete Plant": Art.new(2, 6.0, F, 0, 2, true, Tone.CONCRETE),
		"Panel Works": Art.new(1, 9.0, S, 0, 0, true, Tone.CONCRETE),
		# Bitumen has to be kept hot, so an asphalt plant is a mixing tower with
		# tanks under it rather than a building with a yard.
		"Asphalt Plant": Art.new(2, 7.0, F, 1, 2, false, Tone.METAL),

		# Chemicals and what comes after them. The works is the dirtiest thing in
		# the republic and looks it; the electronics combine is the cleanest piece
		# of industry there is, so it gets the light-industry read instead.
		"Chemical Works": Art.new(3, 6.0, F, 2, 3, false, Tone.METAL),
		"Electronics Combine": Art.new(3, 4.2, S, 0, 0, false, Tone.RENDER),
		# Brick and a stack, with the tanks the fermentation needs.
		"State Distillery": Art.new(2, 5.5, F, 1, 2, false, Tone.BRICK),

		# Power and heat. The tallest stacks in the republic.
		"Coal Power Plant": Art.new(4, 6.5, F, 2, 0, false, Tone.CONCRETE),
		"Oil Power Plant": Art.new(4, 6.5, F, 1, 2, false, Tone.CONCRETE),
		"Heating Plant": Art.new(2, 6.0, F, 1, 0, false, Tone.BRICK),

		# Light industry and agriculture.
		"Collective Farm": Art.new(1, 6.5, P, 0, 2, false, Tone.TIMBER),
		"Food Factory": Art.new(2, 5.0, S, 1, 1, false, Tone.RENDER),
		"Textile Mill": Art.new(3, 4.5, S, 1, 0, false, Tone.BRICK),

		# Civic. Lower, plainer, rendered rather than industrial.
		"State Store": Art.new(2, 4.2, F, 0, 0, false, Tone.RENDER),
		"Polyclinic": Art.new(3, 3.4, F, 0, 0, false, Tone.RENDER),
		"Culture Club": Art.new(2, 5.5, F, 0, 0, false, Tone.RENDER),
		# Education. Three plain floors of classrooms for the school; the
		# institute is taller and has the only civic gantry, which is what makes
		# it read as somewhere technical rather than as a bigger school.
		"Ten-Year School": Art.new(3, 3.6, F, 0, 0, false, Tone.RENDER),
		"Polytechnic Institute": Art.new(4, 3.8, F, 0, 0, true, Tone.CONCRETE),

		# The services. Civic, so rendered rather than industrial, and low
		# except where the real thing is not: a district hospital is a slab
		# block and a sports hall is one tall room. The prison is the only
		# concrete one, and that is the whole of its character.
		"Kindergarten": Art.new(1, 3.4, P, 0, 0, false, Tone.RENDER),
		"District Hospital": Art.new(6, 3.4, F, 1, 0, false, Tone.RENDER),
		"Pharmacy": Art.new(1, 4.0, F, 0, 0, false, Tone.RENDER),
		"Fire Station": Art.new(1, 6.5, F, 0, 0, true, Tone.BRICK),
		"Militia Station": Art.new(2, 3.6, F, 0, 0, false, Tone.RENDER),
		"People's Court": Art.new(2, 5.5, F, 0, 0, false, Tone.RENDER),
		"Corrective Labour Colony": Art.new(2, 3.4, F, 0, 0, false, Tone.CONCRETE),
		"Sports Hall": Art.new(1, 12.0, F, 0, 0, false, Tone.CONCRETE),
		"Cinema": Art.new(1, 9.0, F, 0, 0, false, Tone.RENDER),
		# The only building whose mast is the point of it. No stack and no
		# silo will read as an aerial, so the gantry stands in for the tower.
		"State Radio Centre": Art.new(2, 4.0, F, 0, 0, true, Tone.RENDER),
		# Almost nothing above ground, which is correct.
		"Cemetery": Art.new(1, 2.5, P, 0, 0, false, Tone.BRICK),

		# Utilities. The station is a low box with a gantry -- switchgear reads
		# as a frame rather than as a building -- and the two ways of dealing
		# with rubbish are told apart by the incinerator's stack.
		"Transformer Station": Art.new(1, 5.0, F, 0, 0, true, Tone.CONCRETE),
		"Landfill": Art.new(1, 4.0, F, 0, 0, false, Tone.CONCRETE),
		"Refuse Incinerator": Art.new(2, 7.0, F, 2, 0, false, Tone.METAL),

		# Storage and logistics. Long, low, and mostly roof.
		"Warehouse": Art.new(1, 11.0, F, 0, 0, false, Tone.CONCRETE),
		# The four stores that are not sheds, and each one has to read as the shape
		# of goods it takes -- that is the whole point of storage having forms. A
		# tank is a tank, a silo is a silo, and a heap of gravel is a low bay.
		# The open yard is the flattest thing in the republic on purpose: it is a
		# fence and a hardstanding, and anything taller would read as a building.
		"Open Storage Yard": Art.new(1, 1.6, F, 0, 0, false, Tone.CONCRETE),
		"Aggregate Store": Art.new(1, 4.0, F, 0, 0, false, Tone.CONCRETE),
		"Storage Tank": Art.new(1, 10.0, F, 0, 3, false, Tone.METAL),
		"Grain Silo": Art.new(1, 14.0, F, 0, 4, false, Tone.CONCRETE),
		"Council Depot": Art.new(1, 8.0, F, 0, 0, false, Tone.CONCRETE),
		"Construction Office": Art.new(2, 3.6, F, 0, 0, false, Tone.RENDER),
		"Motor Depot": Art.new(1, 9.0, S, 0, 0, false, Tone.METAL),
		"Gas Station": Art.new(1, 4.0, F, 0, 2, false, Tone.METAL),
		"Bus Depot": Art.new(1, 9.5, S, 0, 0, false, Tone.METAL),

		# Passenger depots. All three are a shed full of vehicles, so they share
		# the bus depot's sawtooth-and-metal read; what tells them apart is size,
		# which comes from the real footprint the simulation authors. The metro
		# depot is the only one with a gantry, because most of it is underground
		# and the surface building is a hoist over a shaft.
		"Trolleybus Depot": Art.new(1, 9.0, S, 0, 0, false, Tone.METAL),
		"Tram Depot": Art.new(1, 10.0, S, 0, 0, false, Tone.METAL),
		"Metro Depot": Art.new(1, 11.0, F, 0, 0, true, Tone.CONCRETE),

		# Terminals. Long, low and mostly canopy -- a station and a wharf are
		# both a shed over a platform, and the gantry is what makes them read as
		# places where something is craned rather than as another warehouse.
		# The aerodrome is the flattest thing in the republic on purpose: it is
		# almost all apron, and a tall one would read as a hangar estate.
		"Railway Station": Art.new(2, 5.0, F, 0, 0, true, Tone.CONCRETE),
		"River Port": Art.new(1, 7.0, F, 0, 1, true, Tone.CONCRETE),
		"Aerodrome": Art.new(1, 6.0, F, 0, 0, false, Tone.CONCRETE),
		"Distribution Office": Art.new(2, 4.0, F, 0, 0, false, Tone.RENDER),

		# Tourism. The tallest civic building there is, and deliberately so: an
		# Intourist hotel was a statement building, and it is the one thing a
		# republic puts up for the benefit of people who do not live there.

		# Tourism. The tallest civic building there is, and deliberately so: an
		# Intourist hotel was a statement building, and it is the one thing a
		# republic puts up for the benefit of people who do not live there.
		"Intourist Hotel": Art.new(8, 3.2, F, 0, 0, false, Tone.RENDER),

		# The frontier.
		"Customs House": Art.new(2, 4.0, F, 0, 0, false, Tone.RENDER),
	}


## Fail on any building the simulation knows about and this table does not.
##
## The kit is what makes a hundred-plus roster affordable, and the way that goes
## wrong is a new building quietly rendering as a default box that nobody chose.
## This turns that into a loud failure at load.
static func check(names: PackedStringArray) -> void:
	var art := table()
	var missing := PackedStringArray()
	for name in names:
		if not art.has(name):
			missing.append(name)
	if not missing.is_empty():
		push_error("building art is unauthored for: %s" % ", ".join(missing))
		assert(false, "every building kind needs a row in building_art.gd")
	# And the other direction: a row for something that no longer exists is a
	# row nobody will notice has gone stale.
	for name in art.keys():
		if not names.has(name):
			push_warning("building art authored for '%s', which no longer exists" % name)
