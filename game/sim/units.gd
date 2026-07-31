## Metric, and honest about it. Metres and seconds.
##
## The Rust build gave each quantity its own newtype so that adding a distance
## to a duration would not compile. GDScript has no such thing, so what survives
## the crossing is the **conversions** — the places a number changes meaning —
## gathered here rather than scattered as bare arithmetic. A `* 3.6` written
## inline somewhere is the smell this file exists to prevent.
##
## # Positions are not `Vector2`
##
## Godot's `Vector2` and `Vector3` hold **32-bit** floats. The determinism rule
## is bit-exactness on `f64`, and map generation must reproduce across machines
## because a shared seed is a promise between players — so a position that lives
## in a `Vector2` has already lost the argument before anything is computed with
## it. Positions in `sim/` are held as parallel `PackedFloat64Array`s of x and y
## and converted to `Vector3` only at the edge, where something is drawn.
##
## The word "tile" does not belong in this vocabulary. The grid describes
## terrain; it is not the unit things are made of.
class_name Units
extends RefCounted

# ---- speed ----

## Kilometres per hour is the unit a human quotes a vehicle in; metres per
## second is what the simulation computes in.
static func kph_to_mps(kph: float) -> float:
	return kph / 3.6

static func mps_to_kph(mps: float) -> float:
	return mps * 3.6

## How long a speed takes to cover a distance, in seconds.
##
## A zero speed is a caller error rather than an infinity. A stationary thing
## never arrives, and returning `INF` here would propagate into arrival times
## and schedules as a silently poisoned number that surfaces somewhere else.
static func time_to_cover(mps: float, metres: float) -> float:
	assert(mps > 0.0, "a zero speed never covers any distance")
	return metres / mps

# ---- durations, in seconds ----

static func minutes(m: float) -> float:
	return m * 60.0

static func hours(h: float) -> float:
	return h * 3600.0

static func days(d: float) -> float:
	return d * 86400.0

static func as_minutes(s: float) -> float:
	return s / 60.0

static func as_hours(s: float) -> float:
	return s / 3600.0

static func as_days(s: float) -> float:
	return s / 86400.0

# ---- geometry ----

## Straight-line distance between two points. Not a travel distance — routing is
## a graph problem and this is the geometry underneath it.
##
## Deliberately `sqrt` and not a hypot. A hypot is a libm compound function and
## is permitted to differ in its last bit between platforms and library
## versions; `sqrt` is exactly rounded by IEEE-754 everywhere. At map scales
## `dx * dx` is nowhere near overflow, so the only thing a hypot would buy is
## the non-determinism. For the same reason this does not go through
## `Vector2.distance_to`, which would also round the inputs to 32 bits first.
static func distance(ax: float, ay: float, bx: float, by: float) -> float:
	var dx := ax - bx
	var dy := ay - by
	return sqrt(dx * dx + dy * dy)

## Squared distance, for when only an ordering is wanted. Exact, and cheaper.
static func distance_squared(ax: float, ay: float, bx: float, by: float) -> float:
	var dx := ax - bx
	var dy := ay - by
	return dx * dx + dy * dy
