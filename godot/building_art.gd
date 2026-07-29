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

		# Storage and logistics. Long, low, and mostly roof.
		"Warehouse": Art.new(1, 11.0, F, 0, 0, false, Tone.CONCRETE),
		"Council Depot": Art.new(1, 8.0, F, 0, 0, false, Tone.CONCRETE),
		"Construction Office": Art.new(2, 3.6, F, 0, 0, false, Tone.RENDER),
		"Motor Depot": Art.new(1, 9.0, S, 0, 0, false, Tone.METAL),
		"Gas Station": Art.new(1, 4.0, F, 0, 2, false, Tone.METAL),
		"Bus Depot": Art.new(1, 9.5, S, 0, 0, false, Tone.METAL),

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
