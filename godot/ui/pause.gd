extends CanvasLayer

## The pause overlay.
##
## Semi-transparent on purpose: a player who pauses to think should still be
## looking at the thing they are thinking about. A full wash would hide the
## republic that prompted the pause.
##
## It does not pause the simulation itself -- `main.gd` sets the speed to zero and
## remembers what it was, because speed is the republic's state and this is a
## screen. A screen that owned the clock would be a second place the clock lives.

const Style := preload("res://ui/theme.gd")

signal resumed
signal save_pressed
signal load_pressed
signal settings_pressed
signal reference_pressed
signal radio_pressed
signal abandon_pressed

var _title: Label = null
var _subtitle: Label = null
var _abandon: Button = null
var _abandon_armed := false


func _ready() -> void:
	layer = 11
	add_child(Style.backdrop(0.72))

	var centre := CenterContainer.new()
	centre.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(centre)

	# `Style.card` and not a bare Panel: see the note on that helper. A Panel here
	# reported zero height, so the whole overlay collapsed to a line.
	var panel := Style.card(Style.PAPER_RAISED, Style.RULE, 22)
	panel.custom_minimum_size = Vector2(360, 0)
	centre.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 7)
	Style.card_body(panel).add_child(column)

	_title = Style.heading("PAUSED", Style.SIZE_HEAD)
	column.add_child(_title)
	_subtitle = Style.small("", Style.INK_DIM)
	column.add_child(_subtitle)

	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 10)
	column.add_child(gap)

	var resume := Style.button("Resume", true)
	resume.pressed.connect(func(): resumed.emit())
	column.add_child(resume)

	var save := Style.button("Save")
	save.pressed.connect(func(): save_pressed.emit())
	column.add_child(save)

	var load_button := Style.button("Load")
	load_button.pressed.connect(func(): load_pressed.emit())
	column.add_child(load_button)

	var reference := Style.button("Reference")
	reference.pressed.connect(func(): reference_pressed.emit())
	column.add_child(reference)

	var radio := Style.button("The State Radio")
	radio.pressed.connect(func(): radio_pressed.emit())
	column.add_child(radio)

	var settings := Style.button("Settings")
	settings.pressed.connect(func(): settings_pressed.emit())
	column.add_child(settings)

	column.add_child(Style.divider())

	# Abandoning throws away everything since the last save, so it confirms in
	# place. It is the one irreversible thing on this panel.
	_abandon = Style.button("Abandon the posting")
	_abandon.pressed.connect(_on_abandon)
	column.add_child(_abandon)


## Name the republic being paused, so the panel is about somewhere.
func show_for(republic: Node) -> void:
	_abandon_armed = false
	_abandon.text = "Abandon the posting"
	var name: String = republic.republic_name()
	_title.text = name.to_upper() if name != "" else "PAUSED"
	_subtitle.text = "%s  ·  %d people  ·  %d buildings" % [
		republic.date_text(), republic.population(), republic.building_count(),
	]


func _on_abandon() -> void:
	if _abandon_armed:
		abandon_pressed.emit()
		return
	_abandon_armed = true
	_abandon.text = "Abandon? Unsaved work is lost"
