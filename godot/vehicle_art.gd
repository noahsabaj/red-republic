extends RefCounted

## Which body each vehicle kind is drawn with, authored per kind.
##
## The same split as `building_art.gd`, for the same reason: the simulation
## authors what a vehicle *does* — capacity, seats, speed, fuel, how it copes
## with soft ground — and nothing about what it looks like. A field no system
## reads has no business being simulation state, so the body lives here and can
## change without recompiling Rust.
##
## `tools/build_vehicles.py` makes the bodies. There are four of them and
## thirteen kinds, which is deliberate: the bodies exist to be told apart at two
## hundred metres, and a distinct mesh per kind would be twelve more assets to
## carry so that a player could fail to notice eleven of them. What has to read
## differently is a lorry from a tanker from a bus from a plough.
##
## **Every kind must have a row.** `check()` fails loudly on an unauthored one
## rather than letting it fall back to a default, because a defaulted vehicle is
## a decision nobody made — the same rule `building_art.gd` holds.

## The bodies in `models/vehicles.glb`.
const LORRY := "lorry"
const TANKER := "tanker"
const COACH := "coach"
const PLOUGH := "plough"


## Body per kind, keyed by the simulation's own name for it.
##
## Keyed by name rather than by index because an index is a position in a Rust
## table and a reordering there would silently repaint the fleet, where a rename
## fails the check below and says so.
static func bodies() -> Dictionary:
	return {
		# Road freight. The tanker is for what has to ride in a drum: a tipper
		# and a flatbed are the same silhouette from above, a tank is not.
		"Lorry": LORRY,
		"Heavy Lorry": LORRY,
		"Recovery Vehicle": LORRY,
		"Snow Plough": PLOUGH,
		# People.
		"Crew Bus": COACH,
		"Coach": COACH,
		"Trolleybus": COACH,
		# Rail, water and air have no bodies of their own yet, and a locomotive
		# drawn as a lorry would be a worse answer than one drawn as a long box.
		# They take the coach, which is the only long body there is, and this
		# comment is the record that it is a stand-in rather than a choice.
		"Locomotive": COACH,
		"Passenger Train": COACH,
		"Tram": COACH,
		"Metro Train": COACH,
		"Barge": TANKER,
		"Freighter": TANKER,
	}


## Fail loudly on a kind nobody authored.
##
## Called once at scene setup. The art guard on the building table caught
## thirteen unauthored kinds that had shipped in M11, and it caught them because
## it asserted rather than defaulting.
static func check(names: PackedStringArray) -> void:
	var table := bodies()
	var missing: Array[String] = []
	for name in names:
		if not table.has(String(name)):
			missing.append(String(name))
	assert(
		missing.is_empty(),
		"unauthored vehicle art for: %s — add a row to vehicle_art.gd" % ", ".join(missing)
	)
