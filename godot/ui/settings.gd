extends CanvasLayer

## The settings screen.
##
## Every control is built from `settings_store.gd`'s `DEFAULTS` table, which is
## the point: a setting is one row of data and gets its widget, its label, its
## default and its persistence from that row. The alternative -- a table of
## defaults over here and a hand-built widget over there -- is two copies of the
## roster, and this project has watched a hand-sized list silently lose a row
## four times running.
##
## A change applies immediately and is written on close. There is no Apply
## button: reversible actions are not gated behind confirmation, and every
## setting here is reversible.
##
## # It is issued by nobody, and says so
##
## The title block names the organ a screen came from. This one is the *player's*
## rather than the republic's -- vsync is not a decision of the Council of
## Ministers -- so it carries the build instead. Inventing a department for it
## would be the one lie on the screen.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")
const Store := preload("res://settings_store.gd")

signal closed

## What each column of the table is worth. The head and every row read the same
## array, which is what keeps them lined up.
const COLUMNS := [["setting", 3.0], ["", 2.0], ["", 0.7]]

var _store: RefCounted = null
var _rows := {}
var _title_label: Label = null
var _paused_label: Label = null


func _ready() -> void:
	layer = 11
	var sheet: Dictionary = Sheet.build(
		self,
		"Settings",
		"Red Republic %s" % Build.version(),
		"Applied as you change them, and written down when you close this."
	)
	_title_label = sheet["title"]
	_paused_label = sheet["organ"]

	var body: VBoxContainer = sheet["body"]
	var list := Parts.scroller(body)
	_build_rows(list)

	Sheet.close_button(sheet["footer"], "RESTORE DEFAULTS", _on_reset)
	var done := Parts.button("DONE", "Primary")
	done.pressed.connect(_on_close)
	sheet["footer"].add_child(done)


func _build_rows(into: VBoxContainer) -> void:
	var section := ""
	var alt := false
	for row in Store.DEFAULTS:
		if row["section"] != section:
			section = row["section"]
			alt = false
			into.add_child(Parts.gap(P.GAP_WIDE))
			into.add_child(Parts.say(String(section).to_upper(), "Stamp"))
			into.add_child(Parts.rule())
		_build_row(into, row, alt)
		alt = not alt


func _build_row(into: VBoxContainer, row: Dictionary, alt: bool) -> void:
	var line := Parts.row(into, alt)
	line.add_child(Parts.cell(Parts.say(String(row["label"]), "Small"), COLUMNS[0][1]))

	var key := String(row["key"])
	match String(row["kind"]):
		"share":
			var slider := HSlider.new()
			slider.min_value = 0.0
			slider.max_value = 1.0
			slider.step = 0.05
			slider.size_flags_vertical = Control.SIZE_SHRINK_CENTER
			line.add_child(Parts.cell(slider, COLUMNS[1][1]))
			var readout := Parts.figure("")
			line.add_child(Parts.cell(readout, COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT))
			slider.value_changed.connect(func(v: float):
				readout.text = "%3d%%" % int(round(v * 100.0))
				_write(key, v)
			)
			_rows[key] = {"control": slider, "readout": readout}
		"switch":
			var box := HBoxContainer.new()
			box.alignment = BoxContainer.ALIGNMENT_BEGIN
			var check := Parts.toggle()
			# A bound method rather than a lambda, and that is a leak fix rather
			# than a style preference: a lambda that captures the very button it
			# is connected to is a reference cycle, which Godot reports as a
			# leaked `ObjectDB` instance at exit.
			check.toggled.connect(_on_switch.bind(key))
			box.add_child(check)
			line.add_child(Parts.cell(box, COLUMNS[1][1]))
			line.add_child(Parts.cell(Parts.say("", "Faint"), COLUMNS[2][1]))
			_rows[key] = {"control": check}
		"choice":
			var choice := OptionButton.new()
			for option in row["options"]:
				choice.add_item(String(option))
			choice.item_selected.connect(func(i: int): _write(key, i))
			line.add_child(Parts.cell(choice, COLUMNS[1][1]))
			line.add_child(Parts.cell(Parts.say("", "Faint"), COLUMNS[2][1]))
			_rows[key] = {"control": choice}


## A ticked box changed: write it, and write the word in it.
func _on_switch(on: bool, key: String) -> void:
	var entry: Dictionary = _rows.get(key, {})
	var control = entry.get("control")
	if control != null:
		control.text = Parts.toggle_word(on)
	_write(key, on)


## Write a value and apply it at once.
##
## Immediate rather than on Done, because a volume slider you cannot hear while
## dragging is a volume slider you have to guess at.
func _write(key: String, value) -> void:
	if _store == null:
		return
	_store.set_value(key, value)
	_store.apply()


func open(store: RefCounted, in_game: bool) -> void:
	_store = store
	# The same screen from the menu and from the pause overlay. The title block
	# says which, because a full-screen settings page with no context is a screen
	# a player can get lost in.
	_title_label.text = "SETTINGS"
	_paused_label.text = (
		("PAUSED  ·  Red Republic %s" % Build.version()) if in_game
		else "Red Republic %s" % Build.version()
	).to_upper()
	_show_stored()


## Push every stored value into its control, without those pushes writing back.
##
## `set_value_no_signal` on the sliders and `set_pressed_no_signal` on the
## switches: without it, populating the screen fires every changed handler, which
## writes the value it just read and calls `apply()` a dozen times. Harmless here
## and the kind of loop that stops being harmless the moment one of these settings
## does something expensive.
func _show_stored() -> void:
	for key in _rows.keys():
		var entry: Dictionary = _rows[key]
		var value = _store.get_value(key)
		var control = entry["control"]
		if control is HSlider:
			control.set_value_no_signal(float(value))
			entry["readout"].text = "%3d%%" % int(round(float(value) * 100.0))
		elif control is OptionButton:
			control.selected = int(value)
		elif control is Button:
			# A ticked box rather than a switch -- see `Parts.toggle`. The label
			# is set here as well as by the toggled handler, because
			# `set_pressed_no_signal` deliberately does not fire it.
			control.set_pressed_no_signal(bool(value))
			control.text = Parts.toggle_word(bool(value))


func _on_reset() -> void:
	# Reads the same DEFAULTS the first run reads, so a reset and a fresh install
	# cannot disagree. That is the whole reason the defaults are in one table.
	_store.reset()
	_store.apply()
	_show_stored()


func _on_close() -> void:
	_store.save_to_disk()
	closed.emit()
