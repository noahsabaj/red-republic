## The balance table crossed from Rust intact, and is shaped the way `sim/`
## expects to read it.
extends TestCase

func _ready_tables() -> String:
	return Tables.load_tables()

## The load-bearing one: every number in the table, hashed by its bits, agrees
## with what the Rust table hashed to. A figure parsed one ulp out, a row
## reordered or a field dropped all change this — and all three are silent
## otherwise.
func test_the_table_survives_the_crossing() -> void:
	var err := _ready_tables()
	expect(err.is_empty(), "load_tables() said: %s" % err)
	expect_eq(Tables.checksum_got, Tables.checksum_expected, "checksum")

func test_the_rosters_are_the_size_the_simulation_had() -> void:
	expect(_ready_tables().is_empty(), "table loads")
	expect_eq(Tables.building_count, 64, "building kinds")
	expect_eq(Tables.resources.size(), 22, "resources")
	expect_eq(Tables.vehicle_count, 13, "vehicle kinds")
	expect_eq(Tables.forms.size(), 5, "forms")
	expect_eq(Tables.needs.size(), 4, "needs")
	expect_eq(Tables.priorities.size(), 3, "priorities")
	expect_eq(Tables.education.size(), 3, "education levels")

## The offset arrays have to close: one more entry than there are buildings, and
## the last entry is the length of the run array. A slice off the end of these
## is silent in GDScript, so this is the check that would catch a mis-built run.
func test_the_variable_length_runs_close() -> void:
	expect(_ready_tables().is_empty(), "table loads")
	var n := Tables.building_count
	for pair: Array in [
		["inputs", Tables.b_in_at, Tables.b_in_res.size()],
		["outputs", Tables.b_out_at, Tables.b_out_res.size()],
		["materials", Tables.b_mat_at, Tables.b_mat_res.size()],
		["sells", Tables.b_sells_at, Tables.b_sells_res.size()],
		["admits", Tables.b_admits_at, Tables.b_admits_form.size()],
		["serves", Tables.b_serves_at, Tables.b_serves_need.size()],
		["vehicles", Tables.b_veh_at, Tables.b_veh_kind.size()],
	]:
		var label: String = pair[0]
		var offsets: PackedInt32Array = pair[1]
		var total: int = pair[2]
		expect_eq(offsets.size(), n + 1, "%s offsets has one entry per building plus a close" % label)
		if offsets.size() == n + 1:
			expect_eq(offsets[n], total, "%s offsets close at the end of the run" % label)

## Every index stored into the table resolved. `find()` returns -1 on a miss, so
## an unrecognised resource or form name would sit in the arrays as -1 and read
## as "the last one" wherever it was used negatively — or crash much later.
func test_no_index_failed_to_resolve() -> void:
	expect(_ready_tables().is_empty(), "table loads")
	for i in Tables.b_in_res.size():
		expect(Tables.b_in_res[i] >= 0, "input resource %d resolved" % i)
	for i in Tables.b_out_res.size():
		expect(Tables.b_out_res[i] >= 0, "output resource %d resolved" % i)
	for i in Tables.b_mat_res.size():
		expect(Tables.b_mat_res[i] >= 0, "material resource %d resolved" % i)
	for i in Tables.b_admits_form.size():
		expect(Tables.b_admits_form[i] >= 0, "admitted form %d resolved" % i)
	for i in Tables.b_serves_need.size():
		expect(Tables.b_serves_need[i] >= 0, "served need %d resolved" % i)
	for i in Tables.b_veh_kind.size():
		expect(Tables.b_veh_kind[i] >= 0, "establishment vehicle %d resolved" % i)
	for b in Tables.building_count:
		expect(Tables.b_priority[b] >= 0, "%s has a priority" % Tables.b_name[b])
		expect(Tables.b_schooling[b] >= 0, "%s has a schooling bar" % Tables.b_name[b])
	for v in Tables.vehicle_count:
		expect(Tables.v_medium[v] >= 0, "%s has a medium" % Tables.v_name[v])

## Spot-check one row against the Rust source by hand, so a checksum that is
## self-consistently wrong — both sides hashing the same mistake — still fails.
## The Coal Mine, read out of `crates/sim/src/building.rs`.
func test_the_coal_mine_reads_the_way_it_did() -> void:
	expect(_ready_tables().is_empty(), "table loads")
	var b := Tables.building_index("CoalMine")
	expect(b >= 0, "there is a CoalMine")
	if b < 0:
		return
	expect_eq(Tables.b_name[b], "Coal Mine", "name")
	expect_eq(Tables.b_workers[b], 14, "workers")
	expect_exact(Tables.b_power_draw[b], 6.0, "power draw")
	expect_exact(Tables.b_labour[b], 200.0, "builder-days")
	expect_exact(Tables.b_storage[b], 60.0, "storage")
	expect_exact(Tables.b_wear[b], 0.03, "wear")
	expect_exact(Tables.b_pollution[b], 2.0, "pollution")
	var outs := Tables.outputs_of(b)
	expect_eq(outs.size(), 1, "one output")
	if outs.size() == 1:
		expect_eq(Tables.resources[outs[0]], "Coal", "it produces coal")
		expect_exact(Tables.output_rates_of(b)[0], 6.0, "six tonnes a day")
	expect_eq(Tables.materials_of(b).size(), 4, "four materials in the bill")
	expect_eq(Tables.inputs_of(b).size(), 0, "a mine eats nothing")

## A value that is not a round number, to prove the crossing keeps precision
## rather than only keeping integers. 15 km/h cross-country is stored as metres
## per second and read back as 15.000000000000002 — and that is the correct
## answer, not a defect to round away.
func test_precision_is_not_quietly_rounded() -> void:
	expect(_ready_tables().is_empty(), "table loads")
	var v := Tables.vehicle_ids.find("Lorry")
	expect(v >= 0, "there is a Lorry")
	if v < 0:
		return
	expect_exact(Tables.v_cross_country_kph[v], 15.000000000000002, "cross-country speed")
	expect_exact(Tables.v_fuel_per_km[v], 0.0003, "fuel per km")
