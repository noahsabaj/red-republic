## The base every test file extends.
##
## In-house rather than a test add-on, because the surface is this file: a
## handful of checks and a list of failures. A dependency would be larger than
## the thing it replaced.
##
## Deliberately **not** Godot's `assert()`. That is compiled out of release
## builds, so a suite built on it reports success by doing nothing the moment
## anybody runs it the way a player would.
class_name TestCase
extends RefCounted

## Failures from the run in progress, as sentences. Empty means the file passed.
var failures: PackedStringArray = PackedStringArray()

## The test currently running, so a failure can say where it came from without
## every check being handed a label.
var _current: String = ""

## Whether the test that is running reached its own last line.
##
## **A test that died half way through has not passed.** A GDScript runtime
## error aborts the function it happens in and hands control back to the caller
## as though the call returned; there are no exceptions to catch. So the suite
## once printed `ok` while four tests crashed on a misspelled method — every
## check they would have made simply never ran.
##
## The mark is cleared here and set by [method done], which every test calls as
## its last statement. A test that never gets there cannot set it, and the
## runner turns the missing mark into a failure.
var _reached_end: bool = false

func _begin(name: String) -> void:
	_current = name
	_reached_end = false

## Call this as the last line of every test.
##
## It exists because nothing else can tell a test that finished from one that
## was cut off. `tests/test_harness.gd` checks that every test method in the
## suite ends with it, so the requirement is enforced rather than remembered.
func done() -> void:
	_reached_end = true

func fail(message: String) -> void:
	failures.append("%s: %s" % [_current, message])

func expect(condition: bool, message: String) -> void:
	if not condition:
		fail(message)

func expect_eq(got: Variant, want: Variant, message: String) -> void:
	if got != want:
		fail("%s — got %s, want %s" % [message, got, want])

## Float equality by **bits**, not by tolerance.
##
## The determinism rule is bit-exactness, so a check with an epsilon in it would
## pass on exactly the drift it exists to catch. Where a test genuinely wants a
## tolerance it says so with [method expect_near] and names the tolerance.
func expect_exact(got: float, want: float, message: String) -> void:
	if got != want or is_nan(got) != is_nan(want):
		fail("%s — got %s, want %s" % [message, _full(got), _full(want)])

func expect_near(got: float, want: float, tolerance: float, message: String) -> void:
	if absf(got - want) > tolerance:
		fail(
			"%s — got %s, want %s (±%s)"
			% [message, _full(got), _full(want), _full(tolerance)]
		)

## A float printed at full precision.
##
## Not `"%.17g"`: GDScript's format specifiers have no `g`, so that string comes
## out **verbatim** with the numbers missing — which is how a bit-exactness
## failure reported itself as `got %.17g, want %.17g` and said nothing at all.
static func _full(v: float) -> String:
	return String.num(v, 17)
