## The generator draws the same stream the Rust one did.
##
## Checked against `tests/data/rng_vectors.json`, dumped from
## `crates/sim/src/bin/vectors.rs`, rather than against numbers written down
## here. Map generation must reproduce across machines — a shared seed is a
## promise between players — and a reimplementation is only as good as the
## evidence that it draws the same stream.
extends TestCase

const VECTORS := "res://tests/data/rng_vectors.json"

func _vectors() -> Dictionary:
	var text := FileAccess.get_file_as_string(VECTORS)
	if text.is_empty():
		fail("%s is missing or empty" % VECTORS)
		return {}
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		fail("%s is not a JSON object" % VECTORS)
		return {}
	return parsed

## Sixteen hex digits to an `int`, by nibble.
##
## Not `String.hex_to_int()`: every value here with the top bit set is above
## `int64` max as an unsigned number, and a parser that clamps rather than wraps
## would quietly turn half the vectors into the same number.
static func _hex(s: String) -> int:
	var v := 0
	for i in s.length():
		v = (v << 4) | ("0123456789abcdef".find(s[i].to_lower()))
	return v

## The bit pattern of a double, read back as the double.
##
## The vectors carry floats as bits rather than as decimals because this
## repository measured that a JSON parser is not necessarily correctly rounded —
## serde_json returned a different value for 91,767 of 200,000 sampled f64s.
## Going through the bits means this test checks the generator and not the
## reader's arithmetic.
static func _hex64(v: int) -> String:
	const DIGITS := "0123456789abcdef"
	var out := ""
	for i in 16:
		out += DIGITS[(v >> ((15 - i) * 4)) & 0xf]
	return out

static func _bits_to_float(bits: int) -> float:
	var b := PackedByteArray()
	b.resize(8)
	b.encode_s64(0, bits)
	return b.decode_double(0)

## The load-bearing one. Five seeds, sixteen draws each, compared as bit
## patterns.
func test_next_u64_matches_the_rust_stream() -> void:
	var v := _vectors()
	if v.is_empty():
		return
	var table: Dictionary = v["next_u64"]
	expect(table.size() >= 5, "the vectors cover several seeds")
	for seed_text: String in table:
		var rng := Rng.from_seed(_hex(seed_text))
		var want: Array = table[seed_text]
		expect(want.size() >= 16, "seed %s has draws to compare" % seed_text)
		for i in want.size():
			expect_eq(_hex64(rng.next_u64()), want[i], "seed %s draw %d" % [seed_text, i])

func test_next_f64_matches_the_rust_stream() -> void:
	var v := _vectors()
	if v.is_empty():
		return
	var table: Dictionary = v["next_f64_bits"]
	for seed_text: String in table:
		var rng := Rng.from_seed(_hex(seed_text))
		var want: Array = table[seed_text]
		expect(want.size() >= 16, "seed %s has float draws to compare" % seed_text)
		for i in want.size():
			expect_exact(
				rng.next_f64(),
				_bits_to_float(_hex(want[i])),
				"seed %s float draw %d" % [seed_text, i]
			)

## The half of `next_bounded` a naive `% n` gets wrong, and the half a signed
## `%` gets wrong: a negative `x` would return a negative index.
func test_next_bounded_matches_the_rust_stream() -> void:
	var v := _vectors()
	if v.is_empty():
		return
	var table: Dictionary = v["next_bounded"]
	for bound_text: String in table:
		var n := int(bound_text)
		var rng := Rng.from_seed(1961)
		var want: Array = table[bound_text]
		expect(want.size() >= 24, "bound %s has draws to compare" % bound_text)
		for i in want.size():
			var got := rng.next_bounded(n)
			expect_eq(got, want[i], "bound %d draw %d" % [n, i])
			expect(got >= 0 and got < n, "bound %d draw %d is in range" % [n, i])

## The save contract. A generator that restores the seed but not the position
## passes every test above and fails this one.
func test_a_saved_position_resumes_the_same_future() -> void:
	var v := _vectors()
	if v.is_empty():
		return
	var r: Dictionary = v["resume"]
	var live := Rng.from_seed(int(r["seed"]))
	for _i in int(r["draws_before"]):
		live.next_u64()

	var saved := live.state()
	var want: Array = r["after"]
	expect(want.size() >= 8, "the resume vector has draws to compare")

	# The live generator goes on to produce the pinned values...
	for i in want.size():
		expect_eq(_hex64(live.next_u64()), want[i], "live draw %d after the save" % i)

	# ...and one wound to the carried position produces exactly the same.
	var restored := Rng.from_seed(0)
	restored.set_state(saved)
	for i in want.size():
		expect_eq(_hex64(restored.next_u64()), want[i], "restored draw %d" % i)

func test_floats_stay_in_the_half_open_unit_interval() -> void:
	var rng := Rng.from_seed(99)
	for _i in 100000:
		var x := rng.next_f64()
		if x < 0.0 or x >= 1.0:
			fail("%s is outside [0, 1)" % String.num(x, 17))
			return

## Not a quality test — a smoke test that the distribution is not visibly
## broken. A generator that returned a constant, or only even numbers, would
## pass every test above and fail this one.
func test_draws_are_roughly_uniform_across_buckets() -> void:
	var rng := Rng.from_seed(2024)
	var buckets := PackedInt32Array()
	buckets.resize(10)
	const N := 200000
	for _i in N:
		var b := int(rng.next_f64() * 10.0)
		buckets[b] += 1
	var expected := N / 10
	for i in 10:
		expect(
			absi(buckets[i] - expected) < expected / 10,
			"bucket %d held %d, expected about %d" % [i, buckets[i], expected]
		)
