## The gate. `godot --headless --path game --script res://tests/harness.gd`.
##
## Replaces `cargo test`, and has to carry the three defences the Rust guards
## carried, because this project has watched a scan go blind and report success:
##
## 1. it **globs** rather than naming files, so a new `tests/test_*.gd` is picked
##    up without editing anything here;
## 2. it has a **floor** under the number of tests found, so a broken directory
##    scan collapses to "nothing to run" and fails rather than passing green;
## 3. it **prints the count**, so a number that quietly drops is visible in a
##    passing log rather than only in a failing one.
##
## Exit code is 1 on any failure, so CI and a person reading the terminal get the
## same answer.
extends SceneTree

## Below this many discovered test methods, assume the scan broke rather than
## that the suite shrank.
const FLOOR: int = 5

const TESTS_DIR := "res://tests"

func _init() -> void:
	var files := _test_files()
	var total := 0
	var failures: PackedStringArray = PackedStringArray()

	for path in files:
		var script: GDScript = load(path)
		if script == null:
			failures.append("%s: could not be loaded" % path)
			continue
		var case: TestCase = script.new()
		var methods: Array[String] = []
		for m in script.get_script_method_list():
			var name: String = m["name"]
			if name.begins_with("test_"):
				methods.append(name)
		methods.sort()
		for name in methods:
			total += 1
			case._begin("%s::%s" % [path.get_file(), name])
			case.call(name)
		failures.append_array(case.failures)

	print("ran %d tests in %d files" % [total, files.size()])

	if total < FLOOR:
		printerr("only %d tests discovered, floor is %d — the scan is broken, not the suite" % [total, FLOOR])
		quit(1)
		return

	if failures.is_empty():
		print("ok")
		quit(0)
		return

	for f in failures:
		printerr("FAIL  %s" % f)
	printerr("%d failure(s)" % failures.size())
	quit(1)

## Every `test_*.gd` under `tests/`, sorted so a run is reproducible.
func _test_files() -> PackedStringArray:
	var out := PackedStringArray()
	var dir := DirAccess.open(TESTS_DIR)
	if dir == null:
		return out
	for name in dir.get_files():
		# Godot hands exported projects `.gd.remap`; strip it so the suite is
		# runnable from an export as well as from source.
		if name.ends_with(".gd.remap"):
			name = name.trim_suffix(".remap")
		if name.begins_with("test_") and name.ends_with(".gd"):
			out.append("%s/%s" % [TESTS_DIR, name])
	out.sort()
	return out
