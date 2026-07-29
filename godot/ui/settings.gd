extends CanvasLayer

## The settings screen.
##
## Every control is built from `settings_store.gd`'s `DEFAULTS` table, which is
## the point: a setting is one row of data and gets its widget, its label, its
## default and its persistence from that row. The alternative -- a table of
## defaults over here and a hand-built widget over there -- is two copies of the
## roster, and this project has watched a hand-sized list of rows silently lose
## one four times running.
##
## A change applies immediately and is written on close. There is no Apply
## button: reversible actions are not gated behind confirmation here, and every
## setting on this screen is reversible.

const Style := preload("res://ui/theme.gd")

signal closed

var _store: RefCounted = null
var _rows := {}
var _title_label: Label = null


func _ready() -> void:
	layer = 11
	add_child(Style.backdrop(0.94))

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 72)
	margin.add_theme_constant_override("margin_right", 72)
	margin.add_theme_constant_override("margin_top", 36)
	margin.add_theme_constant_override("margin_bottom", 28)
	add_child(margin)

	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 12)
	margin.add_child(rows)

	_title_label = Style.heading("SETTINGS", Style.SIZE_TITLE)
	rows.add_child(_title_label)
	rows.add_child(Style.divider())

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	rows.add_child(scroll)

	var body := VBoxContainer.new()
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation", 6)
	scroll.add_child(body)
	_build_rows(body)

	rows.add_child(Style.divider())
	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", 10)

	var reset := Style.button("Restore defaults")
	# Reads the same DEFAULTS the first run reads, so a reset and a fresh install
	# cannot disagree. That is the whole reason the defaults are in one table.
	reset.pressed.connect(_on_reset)
	footer.add_child(reset)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer.add_child(spacer)

	var done := Style.button("Done", true)
	done.pressed.connect(_on_close)
	footer.add_child(done)
	rows.add_child(footer)


func _build_rows(into: VBoxContainer) -> void:
	var Store := preload("res://settings_store.gd")
	var section := ""
	for row in Store.DEFAULTS:
		if row["section"] != section:
			section = row["section"]
			var gap := Control.new()
			gap.custom_minimum_size = Vector2(0, 10)
			into.add_child(gap)
			into.add_child(Style.small(String(section).to_upper(), Style.ACCENT_HOT))
		into.add_child(_build_row(row))


func _build_row(row: Dictionary) -> Control:
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 16)
	line.custom_minimum_size = Vector2(0, 32)

	var label := Style.body(String(row["label"]), Style.INK)
	label.custom_minimum_size = Vector2(320, 0)
	line.add_child(label)

	var key := String(row["key"])
	match String(row["kind"]):
		"share":
			var slider := HSlider.new()
			slider.min_value = 0.0
			slider.max_value = 1.0
			slider.step = 0.05
			slider.custom_minimum_size = Vector2(260, 0)
			slider.size_flags_vertical = Control.SIZE_SHRINK_CENTER
			line.add_child(slider)
			var readout := Style.small("", Style.INK_DIM)
			readout.custom_minimum_size = Vector2(52, 0)
			line.add_child(readout)
			slider.value_changed.connect(func(v: float):
				readout.text = "%d%%" % int(round(v * 100.0))
				_write(key, v)
			)
			_rows[key] = {"control": slider, "readout": readout}
		"switch":
			var check := CheckButton.new()
			check.toggled.connect(func(on: bool): _write(key, on))
			line.add_child(check)
			_rows[key] = {"control": check}
		"choice":
			var choice := OptionButton.new()
			choice.add_theme_font_size_override("font_size", Style.SIZE_SMALL)
			choice.custom_minimum_size = Vector2(180, 0)
			for option in row["options"]:
				choice.add_item(String(option))
			choice.item_selected.connect(func(i: int): _write(key, i))
			line.add_child(choice)
			_rows[key] = {"control": choice}
	return line


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
	# The same screen from the menu and from the pause overlay. The title says
	# which, because a full-screen settings page with no context is a screen a
	# player can get lost in.
	_title_label.text = "SETTINGS" if not in_game else "SETTINGS  ·  paused"
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
			entry["readout"].text = "%d%%" % int(round(float(value) * 100.0))
		elif control is CheckButton:
			control.set_pressed_no_signal(bool(value))
		elif control is OptionButton:
			control.selected = int(value)


func _on_reset() -> void:
	_store.reset()
	_store.apply()
	_show_stored()


func _on_close() -> void:
	_store.save_to_disk()
	closed.emit()
