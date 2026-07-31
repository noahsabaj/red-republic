## The boundary that replaces the crate boundary.
##
## `crates/sim` had zero engine dependencies, and that was a rule rather than an
## accident: it is what kept the shell decision reversible. The shell is no
## longer being chosen, so the reason has changed — but the rule is worth keeping
## on its own merits, and there is now nothing structural to enforce it. A
## `sim/` file and a `ui/` file are both just GDScript in the same project.
##
## So it is enforced here, by reading the source.
##
## # What is forbidden, and why each one
##
## - **Nodes and scenes.** A simulation that reaches for a node cannot be run
##   headless, and the whole test suite and the trajectory runner are headless.
## - **`ui/`.** The interface reads the simulation; the simulation must not know
##   the interface exists.
## - **`Vector2` and `Vector3`.** These hold **32-bit** floats. The determinism
##   rule is bit-exactness on `f64`, and map generation must reproduce across
##   machines because a shared seed is a promise between players. A position
##   that passes through a `Vector2` has been rounded before anything is
##   computed with it, and nothing downstream would ever say so. This is the
##   entry most worth having: the other two announce themselves the moment you
##   run headless, and this one is silent for ever.
## - **Wall-clock time.** `Time.get_ticks_msec()` and friends make a run a
##   function of how busy the machine was. [SimClock] is the only clock.
## - **`randi`/`randf`/`RandomNumberGenerator`.** Godot's global RNG is seeded
##   from the system and is not resumable. [Rng] is the only source of
##   randomness, and it carries its stream position into the save.
##
## This is a **source scan**, and it carries the same three defences the Rust
## guards carried, because this project has watched a scan go blind and report
## success: it globs rather than naming files, it has a floor under the number
## of files found, and it prints what it checked.
extends TestCase

const SIM_DIR := "res://sim"

## Below this many files, assume the scan broke rather than that `sim/` shrank.
const FLOOR: int = 4

## Each entry is a forbidden token and the sentence saying why.
const FORBIDDEN: Array = [
	["Vector2", "positions in sim/ are f64; Vector2 holds 32-bit floats and rounds them"],
	["Vector3", "positions in sim/ are f64; Vector3 holds 32-bit floats and rounds them"],
	["extends Node", "sim/ must run headless, so nothing in it may be a node"],
	["get_node", "sim/ must not reach into a scene tree"],
	["res://ui", "the interface reads the simulation; the simulation must not know it exists"],
	["Time.get_ticks", "a run must not be a function of how busy the machine was — SimClock is the clock"],
	["Time.get_unix", "a run must not be a function of the wall clock — SimClock is the clock"],
	["RandomNumberGenerator", "Rng is the only source of randomness, because it is the only resumable one"],
	["randi(", "Rng is the only source of randomness, because it is the only resumable one"],
	["randf(", "Rng is the only source of randomness, because it is the only resumable one"],
	["randomize(", "Rng is the only source of randomness, because it is the only resumable one"],
]

func test_the_simulation_does_not_reach_for_the_engine() -> void:
	var files := _sim_files()
	expect(
		files.size() >= FLOOR,
		"found %d files under %s, floor is %d — the scan is broken, not sim/" % [files.size(), SIM_DIR, FLOOR]
	)
	for path in files:
		var source := FileAccess.get_file_as_string(path)
		expect(not source.is_empty(), "%s could be read" % path)
		var code := _without_comments(source)
		for entry: Array in FORBIDDEN:
			var token: String = entry[0]
			var reason: String = entry[1]
			expect(
				not code.contains(token),
				"%s uses `%s` — %s" % [path.get_file(), token, reason]
			)

## Comments are stripped before the scan, so that a file may *explain* why it
## does not use `Vector2` without tripping the rule that says so. Both this file
## and `sim/units.gd` do exactly that, and without this the guard would forbid
## its own documentation.
static func _without_comments(source: String) -> String:
	var out := ""
	for line in source.split("\n"):
		var text: String = line
		# Not a real parse: a `#` inside a string literal would be treated as a
		# comment. Nothing in sim/ has one, and the failure mode is a scan that
		# checks less rather than one that passes something forbidden — the
		# token would still have to appear before the `#` to be missed.
		var hash_at := text.find("#")
		if hash_at >= 0:
			text = text.substr(0, hash_at)
		out += text + "\n"
	return out

static func _sim_files() -> PackedStringArray:
	var out := PackedStringArray()
	var dir := DirAccess.open(SIM_DIR)
	if dir == null:
		return out
	for name in dir.get_files():
		if name.ends_with(".gd.remap"):
			name = name.trim_suffix(".remap")
		if name.ends_with(".gd"):
			out.append("%s/%s" % [SIM_DIR, name])
	out.sort()
	return out
