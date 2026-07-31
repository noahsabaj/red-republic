## The calendar, and the fixed timestep under it.
extends TestCase

func test_founding_is_the_first_of_march_nineteen_sixty() -> void:
	var c := SimClock.new()
	expect_eq(c.year(), 1960, "year")
	expect_eq(c.month(), 3, "month")
	expect_eq(c.day(), 1, "day")
	# The archived build's day index put founding at 60, and contract deadlines
	# were absolute indices against it. Ported balance figures assume this
	# origin, so it is pinned rather than derived.
	expect_eq(c.day_index(), SimClock.FOUNDING_DAY_INDEX, "day index at founding")
	expect_exact(c.time_of_day(), 0.0, "time of day at founding")
	done()

func test_a_day_is_exactly_one_thousand_four_hundred_and_forty_ticks() -> void:
	var c := SimClock.new()
	for _i in SimClock.TICKS_PER_DAY - 1:
		c.advance()
	expect_eq(c.days_elapsed(), 0, "not a new day until the boundary")
	expect(not c.is_day_boundary(), "the tick before the boundary is not one")
	c.advance()
	expect_eq(c.days_elapsed(), 1, "the boundary tick starts a new day")
	expect(c.is_day_boundary(), "the boundary tick is one")
	expect_exact(c.time_of_day(), 0.0, "a new day starts at midnight")
	done()

## Fast-forwarding must not diverge from playing. If these ever disagree, a
## republic advanced at speed 5 is a different republic from one advanced at
## speed 1, and nothing else would say so.
func test_advancing_in_bulk_matches_advancing_one_at_a_time() -> void:
	var a := SimClock.new()
	var b := SimClock.new()
	for _i in 5000:
		a.advance()
	b.advance_by(5000)
	expect_eq(a.ticks, b.ticks, "ticks")
	expect_eq(a.day_index(), b.day_index(), "day index")
	expect_exact(a.time_of_day(), b.time_of_day(), "time of day")
	done()

## The calendar round-trips. Every day of a decade, out to a date and back.
func test_a_date_survives_the_round_trip() -> void:
	for index in range(0, 3600):
		var y := SimClock.year_of(index)
		var m := SimClock.month_of(index)
		var d := SimClock.day_of(index)
		expect(m >= 1 and m <= 12, "day %d has a month in range, got %d" % [index, m])
		expect(d >= 1 and d <= 30, "day %d has a day in range, got %d" % [index, d])
		expect_eq(SimClock.day_index_of(y, m, d), index, "day %d round-trips" % index)
	done()

func test_the_year_is_three_hundred_and_sixty_days() -> void:
	expect_eq(SimClock.DAYS_PER_YEAR, 360, "days per year")
	expect_eq(SimClock.year_of(0), 1960, "day 0 is 1960")
	expect_eq(SimClock.year_of(359), 1960, "day 359 is still 1960")
	expect_eq(SimClock.year_of(360), 1961, "day 360 is 1961")
	done()

func test_the_seasons_fall_where_the_calendar_puts_them() -> void:
	for m in [12, 1, 2]:
		expect_eq(SimClock.season_of(m), SimClock.Season.WINTER, "month %d is winter" % m)
	for m in [3, 4, 5]:
		expect_eq(SimClock.season_of(m), SimClock.Season.SPRING, "month %d is spring" % m)
	for m in [6, 7, 8]:
		expect_eq(SimClock.season_of(m), SimClock.Season.SUMMER, "month %d is summer" % m)
	for m in [9, 10, 11]:
		expect_eq(SimClock.season_of(m), SimClock.Season.AUTUMN, "month %d is autumn" % m)
	# Founding is in spring, which is what makes the first winter a deadline
	# rather than an opening condition.
	expect_eq(SimClock.new().season(), SimClock.Season.SPRING, "founding is in spring")
	done()

## The day of year is what the climate curve is a function of, so it has to
## count from January and not from founding.
func test_the_day_of_year_counts_from_january() -> void:
	var c := SimClock.new()
	expect_eq(c.day_of_year(), 60, "founding is the sixtieth day of the year")
	c.advance_by(SimClock.TICKS_PER_DAY * 300)
	expect_eq(c.day_of_year(), 0, "three hundred days later is 1 January")
	expect_eq(c.year(), 1961, "and a new year")
	done()
