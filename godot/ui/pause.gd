extends CanvasLayer

## The pause overlay.
##
## Semi-transparent on purpose: a player who pauses to think should still be
## looking at the thing they are thinking about. A full wash would hide the
## republic that prompted the pause.
##
## **Not a sheet**, for the same reason the menu is not one: it is a card over
## the world rather than a page of paperwork, and a title block on it would cover
## the republic it exists to keep visible. It carries the type and the red rule
## and nothing else.
##
## It does not pause the simulation itself -- `main.gd` sets the speed to zero and
## remembers what it was, because speed is the republic's state and this is a
## screen. A screen that owned the clock would be a second place the clock lives.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

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
	add_child(Sheet.backdrop(0.68))

	var centre := CenterContainer.new()
	centre.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(centre)

	# A `PanelContainer` and not a bare `Panel`: a Panel reports no minimum size
	# from anchored children, so the whole overlay once collapsed to a line with
	# every node present and every label carrying its text. Only the rendered
	# frame showed it.
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(380, 0)
	centre.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 6)
	panel.add_child(column)

	_title = Parts.say("PAUSED", "Section")
	column.add_child(_title)
	_subtitle = Parts.say("", "Small")
	column.add_child(_subtitle)

	var rule := Panel.new()
	rule.custom_minimum_size = Vector2(0, 2)
	var stamp := StyleBoxFlat.new()
	stamp.bg_color = P.RED
	rule.add_theme_stylebox_override("panel", stamp)
	column.add_child(rule)
	column.add_child(Parts.gap(P.GAP_TIGHT))

	var resume := Parts.button("RESUME", "Primary")
	resume.pressed.connect(func(): resumed.emit())
	column.add_child(resume)

	for choice in [
		["SAVE", func(): save_pressed.emit()],
		["LOAD", func(): load_pressed.emit()],
		["REFERENCE", func(): reference_pressed.emit()],
		["THE STATE RADIO", func(): radio_pressed.emit()],
		["SETTINGS", func(): settings_pressed.emit()],
	]:
		var b := Parts.button(String(choice[0]))
		b.pressed.connect(choice[1])
		column.add_child(b)

	column.add_child(Parts.gap(P.GAP_TIGHT))

	# Abandoning throws away everything since the last save, so it confirms in
	# place. It is the one irreversible thing on this panel, and the standing rule
	# is that only irreversible things confirm at all.
	_abandon = Parts.button("ABANDON THE POSTING", "Quiet")
	_abandon.pressed.connect(_on_abandon)
	column.add_child(_abandon)


## Name the republic being paused, so the panel is about somewhere.
func show_for(republic: Republic) -> void:
	_abandon_armed = false
	_abandon.text = "ABANDON THE POSTING"
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
	_abandon.text = "ABANDON? UNSAVED WORK IS LOST"
