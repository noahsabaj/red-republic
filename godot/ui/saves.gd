extends CanvasLayer

## The save and load screen.
##
## One screen for both, because they are the same list with a different verb, and
## two screens would mean two copies of the row layout.
##
## # Every save is listed, including the ones that will not open
##
## A republic written by a build whose format this one cannot read is shown
## greyed, with the reason. Hiding it would be worse: a player whose republic has
## become unopenable needs to be told, not to watch it disappear from a list.
##
## # Overwriting asks, deleting asks, saving does not
##
## The standing rule is not to gate reversible actions behind "are you sure?".
## Saving is reversible -- it makes a new file. Overwriting and deleting destroy a
## republic, so both confirm, in place on the row rather than in a dialog.

const Style := preload("res://ui/theme.gd")
const Saves := preload("res://saves.gd")

signal closed
signal loaded

enum Mode { LOAD, SAVE }

var _republic: Node = null
var _mode: int = Mode.LOAD
var _title: Label = null
var _list: VBoxContainer = null
var _message: Label = null
var _new_save_row: Control = null
var _climate_names := PackedStringArray()
## The row awaiting a second click to confirm, by file name. One at a time: two
## armed rows is a screen where the player has lost track of what a click does.
var _armed := ""
var _armed_action := ""


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

	_title = Style.heading("REPUBLICS", Style.SIZE_TITLE)
	rows.add_child(_title)

	_message = Style.small("", Style.ALARM)
	rows.add_child(_message)
	rows.add_child(Style.divider())

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	rows.add_child(scroll)

	_list = VBoxContainer.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.add_theme_constant_override("separation", 6)
	scroll.add_child(_list)

	rows.add_child(Style.divider())
	var footer := HBoxContainer.new()
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer.add_child(spacer)
	var back := Style.button("Back")
	back.pressed.connect(func(): closed.emit())
	footer.add_child(back)
	rows.add_child(footer)


func open(republic: Node, mode: int) -> void:
	_republic = republic
	_mode = mode
	_armed = ""
	_message.text = ""
	_climate_names = republic.climate_names()
	_title.text = "LOAD A REPUBLIC" if mode == Mode.LOAD else "SAVE THE REPUBLIC"
	refresh()


func refresh() -> void:
	for child in _list.get_children():
		child.queue_free()
	_new_save_row = null

	if _mode == Mode.SAVE:
		_list.add_child(_build_new_save_row())

	var listing: Array = Saves.listing(_republic)
	if listing.is_empty():
		_list.add_child(Style.body(
			"No republics saved yet." if _mode == Mode.LOAD
			else "No earlier saves of this or any republic.",
			Style.INK_FAINT
		))
		return
	for row in listing:
		_list.add_child(_build_row(row))


## The "save as a new file" row, at the top of the save list.
##
## Its own row rather than a button in the footer, so that saving fresh and
## overwriting sit in the same column and read as the same kind of choice.
func _build_new_save_row() -> Control:
	var panel := Style.card(Style.PAPER_RAISED, Style.ACCENT)
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 14)

	var text := Style.body(
		"%s  ·  %s" % [_republic.republic_name(), _republic.date_text()], Style.INK
	)
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(text)

	var save := Style.button("Save as new", true)
	save.pressed.connect(_on_save_new)
	line.add_child(save)

	Style.card_body(panel).add_child(line)
	return panel


func _build_row(row: Dictionary) -> Control:
	var problem: String = row["problem"]
	var panel := Style.card(
		Style.PAPER_SUNK if problem != "" else Style.PAPER_RAISED
	)
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 14)

	var text := VBoxContainer.new()
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text.add_theme_constant_override("separation", 2)

	if problem != "":
		text.add_child(Style.body(String(row["file"]), Style.INK_FAINT))
		text.add_child(Style.small(problem, Style.ALARM))
	else:
		text.add_child(Style.body(String(row["name"]), Style.INK))
		var climate := ""
		var climate_index: int = row["climate"]
		if climate_index >= 0 and climate_index < _climate_names.size():
			climate = String(_climate_names[climate_index])
		text.add_child(Style.small(
			"%s  ·  year %d, day %d  ·  %d people  ·  %s, %d km" % [
				row["date"], 1 + int(row["day"]) / 360, 1 + int(row["day"]) % 360,
				row["population"], climate, row["extent_km"],
			],
			Style.INK_DIM
		))
	line.add_child(text)

	# An unreadable save can still be deleted. That is the only thing left to do
	# with it, and leaving a player no way to clear it out would be worse than
	# not listing it at all.
	if problem == "":
		if _mode == Mode.LOAD:
			var open_button := Style.button("Open")
			open_button.pressed.connect(_on_load.bind(String(row["path"])))
			line.add_child(open_button)
		else:
			var over := Style.button("Overwrite")
			over.pressed.connect(_on_overwrite.bind(String(row["file"]), over))
			line.add_child(over)

	var delete := Style.button("Delete")
	delete.pressed.connect(_on_delete.bind(String(row["file"]), delete))
	line.add_child(delete)

	Style.card_body(panel).add_child(line)
	return panel


func _on_save_new() -> void:
	var file_name: String = Saves.name_for(
		String(_republic.republic_name()), String(_republic.date_text())
	)
	_write(file_name)


## Overwrite, on a second click.
##
## The confirmation is the button relabelling itself rather than a dialog: it
## keeps the thing being destroyed on screen next to the click that destroys it,
## which a modal dialog covers up.
func _on_overwrite(file_name: String, button: Button) -> void:
	if _armed == file_name and _armed_action == "overwrite":
		_write(file_name)
		return
	_arm(file_name, "overwrite", button, "Overwrite?")


func _on_delete(file_name: String, button: Button) -> void:
	if _armed == file_name and _armed_action == "delete":
		var why := DirAccess.remove_absolute(Saves.path_for(file_name))
		if why != OK:
			_message.text = "could not delete %s" % file_name
		_armed = ""
		refresh()
		return
	_arm(file_name, "delete", button, "Delete?")


func _arm(file_name: String, action: String, button: Button, label: String) -> void:
	# Re-listing resets every other armed row, which is what keeps it to one.
	_armed = file_name
	_armed_action = action
	button.text = label
	_message.text = "click again to confirm"
	_message.add_theme_color_override("font_color", Style.ALARM)


func _write(file_name: String) -> void:
	var why: String = _republic.save_to(Saves.path_for(file_name))
	if why != "":
		_message.text = why
		return
	_armed = ""
	_message.text = "saved as %s" % file_name
	_message.add_theme_color_override("font_color", Style.GOOD)
	refresh()


func _on_load(path: String) -> void:
	var why: String = _republic.load_from(path)
	if why != "":
		_message.text = why
		_message.add_theme_color_override("font_color", Style.ALARM)
		return
	loaded.emit()
