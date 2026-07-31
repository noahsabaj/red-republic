## Worldgen reproduces the landscape the seed promises.
##
## Checked in **three layers**, not one. A generated map is tens of thousands of
## cells and the only practical comparison is a hash — but a single red hash
## cannot tell a wrong integer hash from wrong hydrology, and the depression
## fill and the flow accumulation are exactly where the subtle mistakes live.
## The first layer that disagrees is the one to look at.
extends TestCase

const VECTORS := "res://tests/data/rng_vectors.json"

func _terrain_vectors() -> Dictionary:
	var text := FileAccess.get_file_as_string(VECTORS)
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY or not (parsed as Dictionary).has("terrain"):
		fail("%s has no terrain vectors" % VECTORS)
		return {}
	return (parsed as Dictionary)["terrain"]

## Layer 1: the integer lattice hash, including negative coordinates.
##
## This is where GDScript's sign-extending `>>` shows itself: a hash with the
## top bit set comes out wrong in the high bits only, which survives an eyeball
## and changes the whole landscape.
func test_the_cell_hash_matches() -> void:
	var v := _terrain_vectors()
	if v.is_empty():
		done()
		return
	var rows: Array = v["hash_cell"]
	expect(rows.size() >= 6, "there are hash samples to compare")
	for row: Array in rows:
		var x := Bits.from_hex64(row[0])
		var y := Bits.from_hex64(row[1])
		expect_eq(
			Bits.hex64(Terrain.hash_cell(1961, x, y)),
			row[2],
			"hash_cell(1961, %s, %s)" % [row[0], row[1]]
		)
	done()

## Layer 2: the fractal field, compared by bits. Every step of it is integer
## hashing and polynomial interpolation, so it is exact on any IEEE-754 machine
## and a tolerance here would be hiding something.
func test_the_noise_field_matches() -> void:
	var v := _terrain_vectors()
	if v.is_empty():
		done()
		return
	expect(Tables.load_tables().is_empty(), "table loads")
	var rows: Array = v["fractal_noise"]
	expect(rows.size() >= 5, "there are noise samples to compare")
	for row: Array in rows:
		var got := Terrain.fractal_noise(
			1961, float(row[0]), float(row[1]),
			Tables.terrain_feature_size, Tables.terrain_octaves
		)
		expect_exact(
			got,
			_bits_to_float(Bits.from_hex64(row[2])),
			"fractal_noise at (%s, %s)" % [row[0], row[1]]
		)
	done()

## Layer 3: whole generated maps — surfaces, heights and the surface census.
##
## The census is checked as well as the hashes because it is the half a person
## can read. "13,066 grass, 1,091 forest, 0 rock, 243 water" says what kind of
## posting this is; a hash only says whether two runs agree.
func test_generated_maps_match() -> void:
	var v := _terrain_vectors()
	if v.is_empty():
		done()
		return
	expect(Tables.load_tables().is_empty(), "table loads")
	var maps: Array = v["maps"]
	expect(maps.size() >= 6, "there are maps to compare")
	for m: Dictionary in maps:
		var seed := int(m["seed"])
		var extent := float(m["extent"])
		var label := "seed %d over %d m" % [seed, int(extent)]
		var t := Terrain.generate(seed, extent)

		expect_eq(t.cells, int(m["cells"]), "%s: cells a side" % label)
		if t.cells != int(m["cells"]):
			continue

		var counts := PackedInt32Array()
		counts.resize(4)
		for i in t.surface.size():
			counts[t.surface[i]] += 1
		var want_counts: Array = m["counts"]
		for s in 4:
			expect_eq(counts[s], int(want_counts[s]), "%s: surface %d census" % [label, s])

		expect_eq(Bits.hex64(_fnv(t.surface)), m["surface_fnv"], "%s: surfaces" % label)
		expect_eq(
			Bits.hex64(_fnv(t.height.to_byte_array())), m["height_fnv"], "%s: heights" % label
		)
	done()

## A flat map is flat, buildable, and knows where its edges are.
func test_a_flat_map_is_flat() -> void:
	expect(Tables.load_tables().is_empty(), "table loads")
	var t := Terrain.flat(1000.0, 10.0)
	expect_eq(t.cells, 100, "a 1 km map at 10 m cells")
	expect_exact(t.extent(), 1000.0, "extent")
	expect(t.contains(0.0, 0.0), "the origin is on the map")
	expect(t.contains(999.9, 999.9), "just inside the far corner is on the map")
	expect(not t.contains(1000.0, 0.0), "the far edge is off the map")
	expect(not t.contains(-0.1, 0.0), "before the origin is off the map")
	expect_eq(t.surface_at(500.0, 500.0), Terrain.Surface.GRASS, "flat ground is grass")
	expect_exact(t.height_at(500.0, 500.0), 0.0, "flat ground is level")
	expect(t.area_is_buildable(500.0, 500.0, 50.0, 40.0), "a footprint fits on flat grass")
	expect(not t.area_is_buildable(10.0, 10.0, 50.0, 40.0), "a footprint off the edge does not")
	done()

## The check a road order makes. Sampled at half-cell steps so a one-cell river
## cannot be stepped over — without this a gravel road crosses a river at the
## price of gravel.
func test_water_cannot_be_stepped_over() -> void:
	expect(Tables.load_tables().is_empty(), "table loads")
	var t := Terrain.flat(1000.0, 10.0)
	# One cell of water, straight across the path.
	t.set_surface_at(500.0, 500.0, Terrain.Surface.WATER)
	expect(t.crosses_water(100.0, 505.0, 900.0, 505.0), "a run through the cell meets water")
	expect(not t.crosses_water(100.0, 105.0, 900.0, 105.0), "a run elsewhere does not")
	expect(not t.area_is_buildable(500.0, 500.0, 40.0, 40.0), "nothing is built on water")
	done()

static func _bits_to_float(bits: int) -> float:
	var b := PackedByteArray()
	b.resize(8)
	b.encode_s64(0, bits)
	return b.decode_double(0)

static func _fnv(bytes: PackedByteArray) -> int:
	var h := Bits.from_hex64("cbf29ce484222325")
	const PRIME := 0x100000001b3
	for b in bytes:
		h ^= b
		h *= PRIME
	return h
