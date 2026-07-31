## The authored balance table, loaded once and held as parallel arrays.
##
## **Balance lives in data, behaviour lives in systems.** `data/manifest.json`
## is that data; nothing in `sim/` may contain a list of building kinds, because
## a list of ids inside logic is a thing you must remember to edit and what you
## forget lands silently in a fallback.
##
## # Why parallel arrays rather than a class per building
##
## Measured, on this machine, in Godot 4.7.1: the per-tick production pass over
## four hundred buildings runs at 14,781 ticks/sec as parallel `Packed*Array`s,
## 3,232 as objects and 2,800 as dictionaries. A full tick is thirteen such
## passes, and the top speed buys 28,800 in-game seconds a real second — 480
## ticks. Written the obvious object-per-building way the game misses its own
## top speed by half; written this way it clears it with room. The real-time
## thesis is a standing constraint, so this is not a micro-optimisation, it is
## the shape the constraint requires.
##
## Variable-length rows (inputs, outputs, materials, the rest) are held the way
## a sparse matrix is: one flat array of values and an offset array saying where
## each building's run begins. `inputs_of()` slices it.
class_name Tables
extends RefCounted

const MANIFEST := "res://data/manifest.json"

## The checksum the loaded table must reproduce, or the game refuses to start.
## Written by `crates/sim/src/bin/manifest.rs` over the raw bits of every number
## in the table, in one canonical order.
static var checksum_expected: String = ""
static var checksum_got: String = ""

# ---- rosters, in the order the manifest fixes; these indices reach saves ----
static var resources: PackedStringArray = PackedStringArray()
static var resource_names: PackedStringArray = PackedStringArray()
static var resource_form: PackedInt32Array = PackedInt32Array()
static var resource_price_east: PackedFloat64Array = PackedFloat64Array()
static var resource_price_west: PackedFloat64Array = PackedFloat64Array()
static var resource_is_comfort: PackedByteArray = PackedByteArray()
## -1 where the resource is not dug out of anything, else an index into
## [constant MINERALS].
static var resource_from_mineral: PackedInt32Array = PackedInt32Array()
static var forms: PackedStringArray = PackedStringArray()
static var needs: PackedStringArray = PackedStringArray()
static var education: PackedStringArray = PackedStringArray()
static var priorities: PackedStringArray = PackedStringArray()

# ---- buildings, struct-of-arrays ----
static var building_count: int = 0
static var building_ids: PackedStringArray = PackedStringArray()
static var b_name: PackedStringArray = PackedStringArray()
static var b_width: PackedFloat64Array = PackedFloat64Array()
static var b_depth: PackedFloat64Array = PackedFloat64Array()
static var b_workers: PackedInt32Array = PackedInt32Array()
static var b_priority: PackedInt32Array = PackedInt32Array()
static var b_power_draw: PackedFloat64Array = PackedFloat64Array()
static var b_power_output: PackedFloat64Array = PackedFloat64Array()
static var b_heat: PackedFloat64Array = PackedFloat64Array()
static var b_heat_output: PackedFloat64Array = PackedFloat64Array()
static var b_seats: PackedInt32Array = PackedInt32Array()
static var b_labour: PackedFloat64Array = PackedFloat64Array()
static var b_residents: PackedInt32Array = PackedInt32Array()
static var b_storage: PackedFloat64Array = PackedFloat64Array()
static var b_beds: PackedInt32Array = PackedInt32Array()
static var b_wear: PackedFloat64Array = PackedFloat64Array()
static var b_farms: PackedByteArray = PackedByteArray()
static var b_transforms: PackedByteArray = PackedByteArray()
static var b_waste: PackedFloat64Array = PackedFloat64Array()
static var b_pollution: PackedFloat64Array = PackedFloat64Array()
static var b_stores_to_order: PackedByteArray = PackedByteArray()
static var b_schooling: PackedInt32Array = PackedInt32Array()
## -1 where the building teaches nothing, else an index into `TEACHING`.
static var b_teaches: PackedInt32Array = PackedInt32Array()
## -1 where the building taps nothing, else an index into `MINERALS`.
static var b_taps: PackedInt32Array = PackedInt32Array()
## -1 where the building is not a terminal, else an index into `MEDIA`.
static var b_medium: PackedInt32Array = PackedInt32Array()

# ---- variable-length rows, held as value runs plus offsets ----
static var b_in_at: PackedInt32Array = PackedInt32Array()
static var b_in_res: PackedInt32Array = PackedInt32Array()
static var b_in_rate: PackedFloat64Array = PackedFloat64Array()
static var b_out_at: PackedInt32Array = PackedInt32Array()
static var b_out_res: PackedInt32Array = PackedInt32Array()
static var b_out_rate: PackedFloat64Array = PackedFloat64Array()
static var b_mat_at: PackedInt32Array = PackedInt32Array()
static var b_mat_res: PackedInt32Array = PackedInt32Array()
static var b_mat_tonnes: PackedFloat64Array = PackedFloat64Array()
static var b_sells_at: PackedInt32Array = PackedInt32Array()
static var b_sells_res: PackedInt32Array = PackedInt32Array()
static var b_admits_at: PackedInt32Array = PackedInt32Array()
static var b_admits_form: PackedInt32Array = PackedInt32Array()
static var b_serves_at: PackedInt32Array = PackedInt32Array()
static var b_serves_need: PackedInt32Array = PackedInt32Array()
static var b_serves_share: PackedFloat64Array = PackedFloat64Array()
static var b_veh_at: PackedInt32Array = PackedInt32Array()
static var b_veh_kind: PackedInt32Array = PackedInt32Array()
static var b_veh_count: PackedInt32Array = PackedInt32Array()

# ---- vehicles ----
static var vehicle_count: int = 0
static var vehicle_ids: PackedStringArray = PackedStringArray()
static var v_name: PackedStringArray = PackedStringArray()
static var v_role: PackedStringArray = PackedStringArray()
static var v_medium: PackedInt32Array = PackedInt32Array()
static var v_capacity: PackedFloat64Array = PackedFloat64Array()
static var v_seats: PackedInt32Array = PackedInt32Array()
static var v_on_road_kph: PackedFloat64Array = PackedFloat64Array()
static var v_cross_country_kph: PackedFloat64Array = PackedFloat64Array()
static var v_fuel_per_km: PackedFloat64Array = PackedFloat64Array()
static var v_tank: PackedFloat64Array = PackedFloat64Array()
static var v_ground: PackedFloat64Array = PackedFloat64Array()
static var v_load_penalty: PackedFloat64Array = PackedFloat64Array()

## Enum orders that the manifest names but does not enumerate, because they are
## the simulation's own vocabulary rather than balance figures. Index order is
## load-bearing: it reaches saves.
const MEDIA := ["Road", "Rail", "Tram", "Metro", "Water", "Air"]
const MINERALS := ["Coal", "IronOre", "Oil", "Gravel", "Groundwater"]
const TEACHING := ["School", "University"]

static var _loaded: bool = false

## Load the table, verify it, and say what went wrong if it did not.
## Returns an empty string on success.
static func load_tables() -> String:
	if _loaded:
		return ""
	var text := FileAccess.get_file_as_string(MANIFEST)
	if text.is_empty():
		return "%s is missing or empty" % MANIFEST
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		return "%s is not a JSON object" % MANIFEST
	var m: Dictionary = parsed

	resources = _strings(m["resources"])
	forms = _strings(m["forms"])
	needs = _strings(m["needs"])
	education = _strings(m["education"])
	priorities = _strings(m["priorities"])

	var facts: Dictionary = m["resource_facts"]
	resource_names = PackedStringArray()
	resource_form = PackedInt32Array()
	resource_price_east = PackedFloat64Array()
	resource_price_west = PackedFloat64Array()
	resource_is_comfort = PackedByteArray()
	resource_from_mineral = PackedInt32Array()
	for id in resources:
		var f: Dictionary = facts[id]
		resource_names.append(f["name"])
		resource_form.append(forms.find(f["form"]))
		resource_price_east.append(f["price_east"])
		resource_price_west.append(f["price_west"])
		resource_is_comfort.append(1 if f["is_comfort"] else 0)
		resource_from_mineral.append(
			-1 if f["from_mineral"] == null else MINERALS.find(f["from_mineral"])
		)

	# Vehicles first: a building's establishment names vehicle kinds, and the
	# roster has to exist before those names can become indices.
	var vs: Dictionary = m["vehicles"]
	vehicle_ids = _strings(vs.keys())
	vehicle_count = vehicle_ids.size()
	_reset_vehicles()
	for id in vehicle_ids:
		_append_vehicle(vs[id])

	building_ids = _strings(m["building_order"])
	building_count = building_ids.size()
	var bs: Dictionary = m["buildings"]
	_reset_buildings()
	for id in building_ids:
		_append_building(bs[id])

	checksum_expected = m["checksum"]
	checksum_got = _checksum()
	if checksum_got != checksum_expected:
		return (
			"the balance table did not survive the crossing: computed %s, manifest says %s"
			% [checksum_got, checksum_expected]
		)

	_loaded = true
	return ""

# ---- slices into the variable-length rows ----

static func inputs_of(b: int) -> PackedInt32Array:
	return b_in_res.slice(b_in_at[b], b_in_at[b + 1])

static func input_rates_of(b: int) -> PackedFloat64Array:
	return b_in_rate.slice(b_in_at[b], b_in_at[b + 1])

static func outputs_of(b: int) -> PackedInt32Array:
	return b_out_res.slice(b_out_at[b], b_out_at[b + 1])

static func output_rates_of(b: int) -> PackedFloat64Array:
	return b_out_rate.slice(b_out_at[b], b_out_at[b + 1])

static func materials_of(b: int) -> PackedInt32Array:
	return b_mat_res.slice(b_mat_at[b], b_mat_at[b + 1])

static func material_tonnes_of(b: int) -> PackedFloat64Array:
	return b_mat_tonnes.slice(b_mat_at[b], b_mat_at[b + 1])

static func sells_of(b: int) -> PackedInt32Array:
	return b_sells_res.slice(b_sells_at[b], b_sells_at[b + 1])

static func admits_of(b: int) -> PackedInt32Array:
	return b_admits_form.slice(b_admits_at[b], b_admits_at[b + 1])

static func serves_of(b: int) -> PackedInt32Array:
	return b_serves_need.slice(b_serves_at[b], b_serves_at[b + 1])

static func serve_shares_of(b: int) -> PackedFloat64Array:
	return b_serves_share.slice(b_serves_at[b], b_serves_at[b + 1])

static func vehicles_of(b: int) -> PackedInt32Array:
	return b_veh_kind.slice(b_veh_at[b], b_veh_at[b + 1])

static func vehicle_counts_of(b: int) -> PackedInt32Array:
	return b_veh_count.slice(b_veh_at[b], b_veh_at[b + 1])

static func building_index(id: String) -> int:
	return building_ids.find(id)

static func resource_index(id: String) -> int:
	return resources.find(id)

# ---- loading internals ----

static func _strings(v: Variant) -> PackedStringArray:
	var out := PackedStringArray()
	for s in (v as Array):
		out.append(s)
	return out

## Assigned one by one rather than looped over. `Packed*Array` is a value type
## in GDScript, so gathering them into an `Array` to clear in a loop clears
## copies and leaves the originals untouched — which fails silently, with a
## table that looks loaded and is doubled.
static func _reset_buildings() -> void:
	b_name = PackedStringArray()
	b_width = PackedFloat64Array()
	b_depth = PackedFloat64Array()
	b_workers = PackedInt32Array()
	b_priority = PackedInt32Array()
	b_power_draw = PackedFloat64Array()
	b_power_output = PackedFloat64Array()
	b_heat = PackedFloat64Array()
	b_heat_output = PackedFloat64Array()
	b_seats = PackedInt32Array()
	b_labour = PackedFloat64Array()
	b_residents = PackedInt32Array()
	b_storage = PackedFloat64Array()
	b_beds = PackedInt32Array()
	b_wear = PackedFloat64Array()
	b_farms = PackedByteArray()
	b_transforms = PackedByteArray()
	b_waste = PackedFloat64Array()
	b_pollution = PackedFloat64Array()
	b_stores_to_order = PackedByteArray()
	b_schooling = PackedInt32Array()
	b_teaches = PackedInt32Array()
	b_taps = PackedInt32Array()
	b_medium = PackedInt32Array()

	b_in_at = PackedInt32Array([0])
	b_in_res = PackedInt32Array()
	b_in_rate = PackedFloat64Array()
	b_out_at = PackedInt32Array([0])
	b_out_res = PackedInt32Array()
	b_out_rate = PackedFloat64Array()
	b_mat_at = PackedInt32Array([0])
	b_mat_res = PackedInt32Array()
	b_mat_tonnes = PackedFloat64Array()
	b_sells_at = PackedInt32Array([0])
	b_sells_res = PackedInt32Array()
	b_admits_at = PackedInt32Array([0])
	b_admits_form = PackedInt32Array()
	b_serves_at = PackedInt32Array([0])
	b_serves_need = PackedInt32Array()
	b_serves_share = PackedFloat64Array()
	b_veh_at = PackedInt32Array([0])
	b_veh_kind = PackedInt32Array()
	b_veh_count = PackedInt32Array()

static func _append_building(d: Dictionary) -> void:
	b_name.append(d["name"])
	b_width.append(d["width"])
	b_depth.append(d["depth"])
	b_workers.append(d["workers"])
	b_priority.append(priorities.find(d["priority"]))
	b_power_draw.append(d["power_draw"])
	b_power_output.append(d["power_output"])
	b_heat.append(d["heat"])
	b_heat_output.append(d["heat_output"])
	b_seats.append(d["seats"])
	b_labour.append(d["labour"])
	b_residents.append(d["residents"])
	b_storage.append(d["storage"])
	b_beds.append(d["beds"])
	b_wear.append(d["wear"])
	b_farms.append(1 if d["farms"] else 0)
	b_transforms.append(1 if d["transforms"] else 0)
	b_waste.append(d["waste"])
	b_pollution.append(d["pollution"])
	b_stores_to_order.append(1 if d["stores_to_order"] else 0)
	b_schooling.append(education.find(d["schooling"]))
	b_teaches.append(-1 if d["teaches"] == null else TEACHING.find(d["teaches"]))
	b_taps.append(-1 if d["taps"] == null else MINERALS.find(d["taps"]))
	b_medium.append(-1 if d["medium"] == null else MEDIA.find(d["medium"]))

	for pair in d["inputs"]:
		b_in_res.append(resources.find(pair[0]))
		b_in_rate.append(pair[1])
	b_in_at.append(b_in_res.size())
	for pair in d["outputs"]:
		b_out_res.append(resources.find(pair[0]))
		b_out_rate.append(pair[1])
	b_out_at.append(b_out_res.size())
	for pair in d["materials"]:
		b_mat_res.append(resources.find(pair[0]))
		b_mat_tonnes.append(pair[1])
	b_mat_at.append(b_mat_res.size())
	for r in d["sells"]:
		b_sells_res.append(resources.find(r))
	b_sells_at.append(b_sells_res.size())
	for f in d["admits"]:
		b_admits_form.append(forms.find(f))
	b_admits_at.append(b_admits_form.size())
	for pair in d["serves"]:
		b_serves_need.append(needs.find(pair[0]))
		b_serves_share.append(pair[1])
	b_serves_at.append(b_serves_need.size())
	for pair in d["vehicles"]:
		b_veh_kind.append(vehicle_ids.find(pair[0]))
		b_veh_count.append(pair[1])
	b_veh_at.append(b_veh_kind.size())

static func _reset_vehicles() -> void:
	v_name = PackedStringArray()
	v_role = PackedStringArray()
	v_medium = PackedInt32Array()
	v_capacity = PackedFloat64Array()
	v_seats = PackedInt32Array()
	v_on_road_kph = PackedFloat64Array()
	v_cross_country_kph = PackedFloat64Array()
	v_fuel_per_km = PackedFloat64Array()
	v_tank = PackedFloat64Array()
	v_ground = PackedFloat64Array()
	v_load_penalty = PackedFloat64Array()

static func _append_vehicle(d: Dictionary) -> void:
	v_name.append(d["name"])
	v_role.append(d["role"])
	v_medium.append(MEDIA.find(d["medium"]))
	v_capacity.append(d["capacity_t"])
	v_seats.append(d["seats"])
	v_on_road_kph.append(d["on_road_kph"])
	v_cross_country_kph.append(d["cross_country_kph"])
	v_fuel_per_km.append(d["fuel_per_km"])
	v_tank.append(d["tank_t"])
	v_ground.append(d["ground"])
	v_load_penalty.append(d["load_penalty"])

## FNV-1a over the **bits** of every number in the table, in the order
## `crates/sim/src/bin/manifest.rs` fixed. Any drift — a value parsed one ulp
## out, a row reordered, a field dropped — changes this.
static func _checksum() -> String:
	var stream := PackedFloat64Array()
	for r in resources.size():
		stream.append(resource_price_east[r])
		stream.append(resource_price_west[r])
		stream.append(float(resource_is_comfort[r]))
	for b in building_count:
		stream.append(b_width[b])
		stream.append(b_depth[b])
		stream.append(float(b_workers[b]))
		stream.append(b_power_draw[b])
		stream.append(b_power_output[b])
		stream.append(b_heat[b])
		stream.append(b_heat_output[b])
		stream.append(float(b_seats[b]))
		for c in vehicle_counts_of(b):
			stream.append(float(c))
		stream.append_array(input_rates_of(b))
		stream.append_array(output_rates_of(b))
		stream.append_array(material_tonnes_of(b))
		stream.append(b_labour[b])
		stream.append(float(b_residents[b]))
		stream.append(b_storage[b])
		stream.append(float(b_beds[b]))
		stream.append(b_wear[b])
		stream.append(float(b_farms[b]))
		stream.append(float(b_transforms[b]))
		stream.append(b_waste[b])
		stream.append(b_pollution[b])
		stream.append_array(serve_shares_of(b))
		stream.append(float(b_stores_to_order[b]))
	for v in vehicle_count:
		stream.append(v_capacity[v])
		stream.append(float(v_seats[v]))
		stream.append(v_on_road_kph[v])
		stream.append(v_cross_country_kph[v])
		stream.append(v_fuel_per_km[v])
		stream.append(v_tank[v])
		stream.append(v_ground[v])
		stream.append(v_load_penalty[v])
	return _fnv1a_hex(stream.to_byte_array())

## FNV-1a, 64-bit, matching the Rust side byte for byte.
##
## GDScript's `int` is a signed 64-bit integer and its multiply wraps, which is
## the same arithmetic as Rust's `wrapping_mul` on `u64` — the bits agree even
## though the sign does not, and only the bits are ever looked at.
static func _fnv1a_hex(bytes: PackedByteArray) -> String:
	var h: int = (0xcbf29ce4 << 32) | 0x84222325
	const PRIME: int = 0x100000001b3
	for byte in bytes:
		h ^= byte
		h *= PRIME
	return _hex64(h)

## Sixteen hex digits of an `int`'s bit pattern, read as unsigned.
##
## `"%x" % h` will not do: it prints a negative number with a minus sign, and
## every hash with the top bit set is negative here.
static func _hex64(h: int) -> String:
	const DIGITS := "0123456789abcdef"
	var out := ""
	for i in 16:
		var shift := (15 - i) * 4
		var nibble := (h >> shift) & 0xf
		out += DIGITS[nibble]
	return out
