extends CanvasLayer

## The save and load screen: the archive.
##
## One screen for both, because they are the same list with a different verb, and
## two screens would mean two copies of the row layout.
##
## # Every save is listed, including the ones that will not open
##
## A republic written by a build whose format this one cannot read is shown in
## the faint voice, with the reason. Hiding it would be worse: a player whose
## republic has become unopenable needs to be told, not to watch it disappear
## from a list.
##
## # Overwriting asks, deleting asks, saving does not
##
## The standing rule is not to gate reversible actions behind "are you sure?".
## Saving is reversible -- it makes a new file. Overwriting and deleting destroy a
## republic, so both confirm, in place on the row rather than in a dialog: the
## confirmation keeps the thing being destroyed on screen next to the click that
## destroys it, which a modal covers up.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")
const Saves := preload("res://saves.gd")

signal closed
signal loaded

enum Mode { LOAD, SAVE }

## The archive's columns. Head and rows read the same array.
const COLUMNS := [
	["republic", 3.0],
	["taken", 2.2],
	["people", 1.0, HORIZONTAL_ALIGNMENT_RIGHT],
	["land", 2.0],
	["", 2.4, HORIZONTAL_ALIGNMENT_RIGHT],
]

var _republic: Republic = null
var _mode: int = Mode.LOAD
var _title: Label = null
var _list: VBoxContainer = null
var _notice: Label = null
var _climate_names := PackedStringArray()
## The row awaiting a second click to confirm, by file name. One at a time: two
## armed rows is a screen where the player has lost track of what a click does.
var _armed := ""
var _armed_action := ""
## The format stamp in the footer. See `_ready` for why it is there.
var _format: Label = null


func _ready() -> void:
	layer = 11
	var sheet: Dictionary = Sheet.build(
		self,
		"The Archive",
		"Central Archive of the Republic",
		"Every republic this build has written down, and every one it can no longer read."
	)
	_title = sheet["title"]
	_notice = sheet["notice"]

	var body: VBoxContainer = sheet["body"]
	var table := VBoxContainer.new()
	table.size_flags_vertical = Control.SIZE_EXPAND_FILL
	table.add_theme_constant_override("separation", 0)
	body.add_child(table)
	Parts.head(table, COLUMNS)
	_list = Parts.scroller(table)

	Sheet.close_button(sheet["footer"], "BACK", func(): closed.emit())

	# **What this build reads, stamped on the archive itself.**
	# Every save file in the folder carries its format in its name, and a row
	# that will not open says which format it is -- both of which are numbers
	# with nothing to compare against unless the screen says what *this* build
	# is. It is the one fact on this screen that is about the build rather than
	# about a republic, which is why it sits in the footer and not in the list.
	_format = Parts.say("", "Stamp")
	sheet["footer"].add_child(_format)


func open(republic: Republic, mode: int) -> void:
	_republic = republic
	_mode = mode
	_armed = ""
	_notice.text = ""
	_climate_names = republic.climate_names()
	_title.text = "THE ARCHIVE" if mode == Mode.LOAD else "FILE THE REPUBLIC"
	_format.text = "SAVE FORMAT %d" % republic.save_version()
	refresh()


func refresh() -> void:
	for child in _list.get_children():
		child.queue_free()

	if _mode == Mode.SAVE:
		_build_new_save_row()

	var listing: Array = Saves.listing(_republic)
	if listing.is_empty():
		_list.add_child(Parts.gap(P.GAP_WIDE))
		_list.add_child(Parts.say(
			"Nothing in the archive yet." if _mode == Mode.LOAD
			else "No earlier file of this or any republic.",
			"Faint"
		))
		return
	var alt := false
	for row in listing:
		_build_row(row, alt)
		alt = not alt


## The "file it fresh" row, at the top of the save list.
##
## Its own row rather than a button in the footer, so filing fresh and
## overwriting sit in the same column and read as the same kind of choice.
func _build_new_save_row() -> void:
	var line := Parts.row(_list)
	line.add_child(Parts.cell(
		Parts.say(String(_republic.republic_name()), "Small"), COLUMNS[0][1]
	))
	line.add_child(Parts.cell(
		Parts.figure(String(_republic.date_text())), COLUMNS[1][1], HORIZONTAL_ALIGNMENT_LEFT
	))
	line.add_child(Parts.cell(
		Parts.figure(str(_republic.population())), COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT
	))
	line.add_child(Parts.cell(Parts.say("as it stands now", "Faint"), COLUMNS[3][1]))

	var actions := HBoxContainer.new()
	actions.alignment = BoxContainer.ALIGNMENT_END
	var save := Parts.button("FILE IT", "Primary")
	save.pressed.connect(_on_save_new)
	actions.add_child(save)
	line.add_child(Parts.cell(actions, COLUMNS[4][1]))


func _build_row(row: Dictionary, alt: bool) -> void:
	var problem: String = row["problem"]
	var line := Parts.row(_list, alt)

	if problem != "":
		line.add_child(Parts.cell(Parts.say(String(row["file"]), "Faint"), COLUMNS[0][1]))
		var why := Parts.say(problem, "Alarm")
		why.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		line.add_child(Parts.cell(
			why, COLUMNS[1][1] + COLUMNS[2][1] + COLUMNS[3][1]
		))
	else:
		line.add_child(Parts.cell(Parts.say(String(row["name"]), "Small"), COLUMNS[0][1]))
		line.add_child(Parts.cell(
			Parts.figure("%s  (y%d d%d)" % [
				row["date"], 1 + int(row["day"]) / 360, 1 + int(row["day"]) % 360,
			]),
			COLUMNS[1][1],
			HORIZONTAL_ALIGNMENT_LEFT
		))
		line.add_child(Parts.cell(
			Parts.figure(str(row["population"])), COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT
		))
		var climate := ""
		var climate_index: int = row["climate"]
		if climate_index >= 0 and climate_index < _climate_names.size():
			climate = String(_climate_names[climate_index])
		line.add_child(Parts.cell(
			Parts.say("%s, %d km" % [climate, row["extent_km"]], "Faint"), COLUMNS[3][1]
		))

	var actions := HBoxContainer.new()
	actions.alignment = BoxContainer.ALIGNMENT_END
	actions.add_theme_constant_override("separation", P.GAP_TIGHT)

	# An unreadable save can still be deleted. That is the only thing left to do
	# with it, and leaving a player no way to clear it out would be worse than not
	# listing it at all.
	if problem == "":
		if _mode == Mode.LOAD:
			var open_button := Parts.button("OPEN", "Primary")
			open_button.pressed.connect(_on_load.bind(String(row["path"])))
			actions.add_child(open_button)
		else:
			var over := Parts.button("OVERWRITE")
			over.pressed.connect(_on_overwrite.bind(String(row["file"]), over))
			actions.add_child(over)

	var delete := Parts.button("DELETE", "Quiet")
	delete.pressed.connect(_on_delete.bind(String(row["file"]), delete))
	actions.add_child(delete)
	line.add_child(Parts.cell(actions, COLUMNS[4][1]))


func _on_save_new() -> void:
	_write(Saves.name_for(
		String(_republic.republic_name()),
		String(_republic.date_text()),
		_republic.save_version()
	))


func _on_overwrite(file_name: String, button: Button) -> void:
	if _armed == file_name and _armed_action == "overwrite":
		_write(file_name)
		return
	_arm(file_name, "overwrite", button, "OVERWRITE?")


func _on_delete(file_name: String, button: Button) -> void:
	if _armed == file_name and _armed_action == "delete":
		var why := DirAccess.remove_absolute(Saves.path_for(file_name))
		if why != OK:
			_say("could not delete %s" % file_name, P.ALARM)
		_armed = ""
		refresh()
		return
	_arm(file_name, "delete", button, "DELETE?")


func _arm(file_name: String, action: String, button: Button, label: String) -> void:
	# Re-listing resets every other armed row, which is what keeps it to one.
	_armed = file_name
	_armed_action = action
	button.text = label
	_say("click again to confirm", P.ALARM)


func _write(file_name: String) -> void:
	var why: String = _republic.save_to(Saves.path_for(file_name))
	if why != "":
		_say(why, P.ALARM)
		return
	_armed = ""
	refresh()
	_say("filed as %s" % file_name, P.GOOD)


func _on_load(path: String) -> void:
	var why: String = _republic.load_from(path)
	if why != "":
		_say(why, P.ALARM)
		return
	loaded.emit()


## The one notice line the sheet gives every screen.
##
## The colour is set from the palette rather than by swapping the type variation,
## because this line legitimately says two different things -- a refusal and a
## receipt -- and the difference between them is exactly a colour.
func _say(text: String, tone: Color) -> void:
	_notice.text = text
	_notice.add_theme_color_override("font_color", tone)
