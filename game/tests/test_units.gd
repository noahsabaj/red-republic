## The conversions, and the two rules about how geometry is computed.
extends TestCase

func test_speed_converts_both_ways() -> void:
	expect_exact(Units.kph_to_mps(3.6), 1.0, "3.6 km/h is a metre a second")
	expect_exact(Units.mps_to_kph(1.0), 3.6, "a metre a second is 3.6 km/h")
	# 50 km/h out and back. Not exact in binary, which is the point of checking:
	# the round trip is what the vehicle table relies on.
	expect_near(Units.mps_to_kph(Units.kph_to_mps(50.0)), 50.0, 1e-12, "50 km/h round-trips")

func test_time_to_cover_is_distance_over_speed() -> void:
	expect_exact(Units.time_to_cover(10.0, 100.0), 10.0, "100 m at 10 m/s")
	expect_exact(Units.time_to_cover(Units.kph_to_mps(54.0), 900.0), 60.0, "900 m at 54 km/h is a tick")

func test_durations_convert_both_ways() -> void:
	expect_exact(Units.minutes(1.0), 60.0, "a minute")
	expect_exact(Units.hours(1.0), 3600.0, "an hour")
	expect_exact(Units.days(1.0), 86400.0, "a day")
	expect_exact(Units.as_hours(Units.hours(7.0)), 7.0, "hours round-trip")
	expect_exact(Units.as_days(Units.days(365.0)), 365.0, "days round-trip")

func test_distance_is_the_pythagorean_one() -> void:
	expect_exact(Units.distance(0.0, 0.0, 3.0, 4.0), 5.0, "the 3-4-5 triangle")
	expect_exact(Units.distance(1.0, 1.0, 1.0, 1.0), 0.0, "a point is no distance from itself")
	expect_exact(Units.distance_squared(0.0, 0.0, 3.0, 4.0), 25.0, "squared distance")

## The determinism rule, made concrete.
##
## `Vector2` holds 32-bit floats, so routing a position through one rounds it
## before anything is computed. At map scale — a 6 km posting, positions in
## metres — that rounding is visible in the seventh digit, which is exactly
## where a bit-exact save stops being bit-exact. This test is what says the
## difference is real rather than theoretical.
func test_a_vector2_would_lose_the_position() -> void:
	var x := 5432.109876543210
	var y := 1234.567890123456
	var through_vector2 := float(Vector2(x, y).x)
	expect(
		through_vector2 != x,
		"a Vector2 must lose precision here, or this test is checking nothing"
	)
	# And the f64 path keeps it exactly.
	var kept := Units.distance(x, y, x, y)
	expect_exact(kept, 0.0, "the f64 path is exact")
	# The distance a Vector2 round trip would report between a point and itself
	# after rounding — nonzero, which is the failure it would introduce.
	var rounded_x := float(Vector2(x, y).x)
	expect(
		Units.distance(x, 0.0, rounded_x, 0.0) > 0.0,
		"rounding through a Vector2 moves the point"
	)
