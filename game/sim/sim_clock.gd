## Simulated time: a fixed timestep, and the calendar derived from it.
##
## # The tick
##
## The simulation advances in whole ticks of [constant TICK] and never in
## variable real-time deltas. A fixed timestep is not a style preference — it is
## what makes a run reproducible, because a variable step makes the result a
## function of how busy the machine was.
##
## **How many ticks pass per real second is not this file's business.** That is
## a game-speed control and it belongs to the shell. The simulation only knows
## how to take one step.
##
## # The calendar
##
## Twelve thirty-day months, a 360-day year, founding on 1 March 1960. It is a
## simplification, deliberately: equal months make seasonal balance legible, and
## every balance figure in `data/manifest.json` was tuned against this calendar.
## Changing it would silently invalidate all of them.
##
## Named `SimClock` rather than `Clock` because Godot has a `Time` singleton and
## a class called `Clock` beside it reads as the wall clock, which is the one
## thing this must never be.
class_name SimClock
extends RefCounted

## The fixed simulation timestep, in seconds — one simulated minute.
##
## Fine enough that a commute is felt (a lorry at 54 km/h covers 900 m a step)
## and coarse enough that a day is 1,440 steps rather than tens of thousands.
const TICK: float = 60.0

## Ticks in one simulated day.
const TICKS_PER_DAY: int = 1440

const DAYS_PER_MONTH: int = 30
const MONTHS_PER_YEAR: int = 12
const DAYS_PER_YEAR: int = DAYS_PER_MONTH * MONTHS_PER_YEAR

## The year day indices count from. Day index 0 is 1 January 1960.
const EPOCH_YEAR: int = 1960

## Founding is 1 March 1960 — day index 60, not 0. The archived build's
## contract deadlines were absolute indices against this origin, and the ported
## balance figures assume it.
const FOUNDING_DAY_INDEX: int = 60

enum Season { WINTER, SPRING, SUMMER, AUTUMN }

## Ticks elapsed since founding. Monotonic, and the single source of "when it
## is" — nothing else may keep its own notion of elapsed time.
var ticks: int = 0

## Take one step. The only way time moves.
func advance() -> void:
	ticks += 1

## Take several steps at once. Identical in effect to calling [method advance]
## that many times — it must stay that way, or fast-forwarding would diverge
## from playing.
func advance_by(n: int) -> void:
	ticks += n

## Simulated seconds since founding.
func elapsed() -> float:
	return TICK * float(ticks)

## Whole days since founding.
func days_elapsed() -> int:
	return ticks / TICKS_PER_DAY

## Days since 1 January 1960 — the index balance tables and deadlines use.
func day_index() -> int:
	return FOUNDING_DAY_INDEX + days_elapsed()

## Days since 1 January of the current year — what the climate curve is a
## function of.
func day_of_year() -> int:
	return day_index() % DAYS_PER_YEAR

## Seconds since midnight. Sub-day resolution is the reason the tick is a minute
## rather than a day: a commute that happens at a time of day is the whole point
## of simulating citizens individually.
func time_of_day() -> float:
	return TICK * float(ticks % TICKS_PER_DAY)

## True on the tick that starts a new day — where daily bookkeeping hangs.
func is_day_boundary() -> bool:
	return ticks % TICKS_PER_DAY == 0

func year() -> int:
	return year_of(day_index())

func month() -> int:
	return month_of(day_index())

func day() -> int:
	return day_of(day_index())

func season() -> Season:
	return season_of(month())

# ---- the calendar, as free functions on a day index ----
#
# Returned as three integers rather than a `Date` object: a date is asked for
# once a frame by the interface and allocating a `RefCounted` to answer would be
# an object per query for three numbers.

static func year_of(index: int) -> int:
	return EPOCH_YEAR + index / DAYS_PER_YEAR

static func month_of(index: int) -> int:
	return (index % DAYS_PER_YEAR) / DAYS_PER_MONTH + 1

static func day_of(index: int) -> int:
	return (index % DAYS_PER_YEAR) % DAYS_PER_MONTH + 1

static func day_index_of(y: int, m: int, d: int) -> int:
	return (y - EPOCH_YEAR) * DAYS_PER_YEAR + (m - 1) * DAYS_PER_MONTH + (d - 1)

static func season_of(m: int) -> Season:
	if m == 12 or m == 1 or m == 2:
		return Season.WINTER
	if m >= 3 and m <= 5:
		return Season.SPRING
	if m >= 6 and m <= 8:
		return Season.SUMMER
	return Season.AUTUMN
