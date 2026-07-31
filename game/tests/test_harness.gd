## The suite checks itself.
##
## A test framework is a thing that reports success, so it is exactly the thing
## that must not be able to report success wrongly. This project has watched a
## scan go blind and a guard pass by doing nothing; the suite gets the same
## suspicion.
##
## What it enforces is the convention that makes an aborted test visible. A
## GDScript runtime error cuts the function short and hands control back as
## though it returned — there are no exceptions — so the runner cannot tell a
## test that finished from one that died on its third line. Every test therefore
## ends with `done()`, and `harness.gd` turns a missing mark into a failure.
##
## That only works if every test really does call it, which is the sort of rule
## that rots the first time somebody adds a test in a hurry. So it is read out of
## the source here rather than left to habit.
extends TestCase

const TESTS_DIR := "res://tests"

## Below this many test methods, assume the scan broke rather than that the suite
## shrank. The real population was 35 when this was written.
const FLOOR: int = 20

func test_every_test_ends_by_saying_so() -> void:
	var found := 0
	for path in _test_files():
		var source := FileAccess.get_file_as_string(path)
		expect(not source.is_empty(), "%s could be read" % path)
		var lines := source.split("\n")
		var name := ""
		var last := ""
		for i in lines.size():
			var line: String = lines[i]
			var top_level := not line.is_empty() and not line.begins_with("\t")
			if top_level and not name.is_empty():
				_check(path, name, last)
				name = ""
			if top_level and line.begins_with("func test_"):
				name = line.substr(5, line.find("(") - 5)
				found += 1
				last = ""
			elif not name.is_empty() and not line.strip_edges().is_empty():
				last = line.strip_edges()
		if not name.is_empty():
			_check(path, name, last)

	expect(
		found >= FLOOR,
		"found %d test methods, floor is %d — the scan is broken, not the suite" % [found, FLOOR]
	)
	done()

func _check(path: String, name: String, last: String) -> void:
	expect(
		last == "done()",
		"%s::%s must end with done(), or an abort in it reads as a pass — ends with `%s`"
		% [path.get_file(), name, last]
	)

## Every `return` inside a test has to be preceded by the mark too, or a guard
## clause taking an early exit would report itself as an abort.
func test_every_early_return_says_so_first() -> void:
	for path in _test_files():
		var source := FileAccess.get_file_as_string(path)
		var lines := source.split("\n")
		var name := ""
		var previous := ""
		for i in lines.size():
			var line: String = lines[i]
			var trimmed: String = line.strip_edges()
			var top_level := not line.is_empty() and not line.begins_with("\t")
			if top_level:
				name = line.substr(5, line.find("(") - 5) if line.begins_with("func test_") else ""
			elif not name.is_empty() and trimmed == "return":
				expect(
					previous == "done()",
					"%s::%s returns early without done() first, which reads as an abort"
					% [path.get_file(), name]
				)
			if not trimmed.is_empty():
				previous = trimmed
	done()

static func _test_files() -> PackedStringArray:
	var out := PackedStringArray()
	var dir := DirAccess.open(TESTS_DIR)
	if dir == null:
		return out
	for name in dir.get_files():
		if name.ends_with(".gd.remap"):
			name = name.trim_suffix(".remap")
		if name.begins_with("test_") and name.ends_with(".gd"):
			out.append("%s/%s" % [TESTS_DIR, name])
	out.sort()
	return out
