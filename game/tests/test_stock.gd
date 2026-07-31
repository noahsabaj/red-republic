## Stockpiles clamp at zero, and the flat layout addresses the right slot.
extends TestCase

func _setup() -> bool:
	return Tables.load_tables().is_empty()

func test_a_fresh_stockpile_is_empty() -> void:
	expect(_setup(), "table loads")
	var s := Stock.make(4)
	expect_eq(s.size(), 4 * Tables.resources.size(), "one run per building")
	for b in 4:
		expect(Stock.is_empty(s, b), "building %d starts empty" % b)
		expect_exact(Stock.total(s, b), 0.0, "building %d holds nothing" % b)

## The layout check. A flat array addressed with the wrong stride is the defect
## that puts a mine's coal into the building next door, and it reads as a
## balance problem for weeks.
func test_each_building_holds_its_own() -> void:
	expect(_setup(), "table loads")
	var coal := Tables.resource_index("Coal")
	var steel := Tables.resource_index("Steel")
	var s := Stock.make(3)
	Stock.add(s, 1, coal, 50.0)
	expect_exact(Stock.get_at(s, 1, coal), 50.0, "the coal landed where it was put")
	expect_exact(Stock.get_at(s, 0, coal), 0.0, "the building before has none")
	expect_exact(Stock.get_at(s, 2, coal), 0.0, "the building after has none")
	expect_exact(Stock.get_at(s, 1, steel), 0.0, "and no steel appeared")
	expect_exact(Stock.total(s, 1), 50.0, "the total is the one holding")

## The rule: a shortfall is a smaller delivery, not a debt.
func test_stock_never_goes_negative() -> void:
	expect(_setup(), "table loads")
	var coal := Tables.resource_index("Coal")
	var s := Stock.make(1)
	Stock.add(s, 0, coal, 10.0)

	var got := Stock.take(s, 0, coal, 4.0)
	expect_exact(got, 4.0, "took what was asked for")
	expect_exact(Stock.get_at(s, 0, coal), 6.0, "and left the rest")

	got = Stock.take(s, 0, coal, 100.0)
	expect_exact(got, 6.0, "took only what was there")
	expect_exact(Stock.get_at(s, 0, coal), 0.0, "and left nothing")
	expect(not Stock.has(s, 0, coal), "an empty bin has nothing")

	# Subtracting past zero clamps rather than owing.
	Stock.add(s, 0, coal, -50.0)
	expect_exact(Stock.get_at(s, 0, coal), 0.0, "a negative add clamps at zero")
	Stock.set_at(s, 0, coal, -1.0)
	expect_exact(Stock.get_at(s, 0, coal), 0.0, "a negative set clamps at zero")

func test_a_row_reads_back_in_resource_order() -> void:
	expect(_setup(), "table loads")
	var s := Stock.make(2)
	for r in Tables.resources.size():
		Stock.add(s, 1, r, float(r) + 1.0)
	var row := Stock.row(s, 1)
	expect_eq(row.size(), Tables.resources.size(), "a row is one slot per resource")
	for r in row.size():
		expect_exact(row[r], float(r) + 1.0, "slot %d" % r)
	expect_exact(Stock.total(s, 0), 0.0, "the other building is untouched")
