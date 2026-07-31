extends CanvasLayer

## The main menu: the first thing a stranger sees.
##
## **The one screen that is not a form**, and deliberately: it is the cover of
## the file rather than a page inside it. So it keeps the interface's type,
## colour and squareness and drops the title block, because a cover with a
## departmental stamp on it would be a page.
##
## Built in code rather than authored as a `.tscn`, which is the standing choice
## for every screen here. A hand-written scene file is a second language to get
## wrong with no compiler behind it, and the thing this project has learned
## repeatedly about Godot is that a mistake in scene setup fails *silently* --
## geometry wound the wrong way renders as nothing, a `class_name` needs an
## import pass, a node path typo is a null at runtime. Code gets parsed.
##
## Nothing here styles anything. `ui/theme.tres` is the project's default theme,
## so a `Button` is already a Red Republic button before this file touches it.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal new_posting_pressed
signal continue_pressed
signal load_pressed
signal settings_pressed
signal reference_pressed
signal quit_pressed

## How wide the column of choices is. Wide enough that the longest label does not
## crowd its box, and narrow enough that the eye reads it as a list rather than
## as a row of banners.
const COLUMN := 340

var _continue_button: Button = null
var _continue_note: Label = null


func _ready() -> void:
	layer = 10
	add_child(Sheet.backdrop())

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 96)
	margin.add_theme_constant_override("margin_right", 96)
	margin.add_theme_constant_override("margin_top", 72)
	margin.add_theme_constant_override("margin_bottom", 56)
	add_child(margin)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", P.GAP)
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	column.custom_minimum_size = Vector2(COLUMN, 0)
	margin.add_child(column)

	# The title carries the fiction and the line under it carries the thesis. A
	# stranger should be able to read what kind of game this is off this screen
	# without being told: there is no tutorial and no advisor to explain it later.
	column.add_child(Parts.say("RED REPUBLIC", "Title"))

	# The red rule is the interface's signature, and this is the one place it is
	# the whole of the decoration.
	var rule := Panel.new()
	rule.custom_minimum_size = Vector2(COLUMN, 3)
	var stamp := StyleBoxFlat.new()
	stamp.bg_color = P.RED
	rule.add_theme_stylebox_override("panel", stamp)
	column.add_child(rule)

	column.add_child(Parts.say("A planned economy, one second at a time.", "Body"))
	column.add_child(Parts.gap(P.GAP_SECTION))

	var buttons := VBoxContainer.new()
	buttons.add_theme_constant_override("separation", 6)
	column.add_child(buttons)

	# Continue sits first and is the primary action once there is something to
	# continue, because a returning player is the common case and a new posting is
	# the rare one. On a first run it is disabled with a note saying why, rather
	# than hidden -- a menu whose items move between runs is a menu a player has to
	# re-read.
	_continue_button = Parts.button("CONTINUE", "Primary")
	_continue_button.pressed.connect(func(): continue_pressed.emit())
	buttons.add_child(_continue_button)

	_continue_note = Parts.say("", "Faint")
	buttons.add_child(_continue_note)
	buttons.add_child(Parts.gap(P.GAP_TIGHT))

	for choice in [
		["TAKE A NEW POSTING", func(): new_posting_pressed.emit()],
		["LOAD A REPUBLIC", func(): load_pressed.emit()],
	]:
		var b := Parts.button(String(choice[0]))
		b.pressed.connect(choice[1])
		buttons.add_child(b)

	buttons.add_child(Parts.gap(P.GAP))

	for choice in [
		["REFERENCE", func(): reference_pressed.emit()],
		["SETTINGS", func(): settings_pressed.emit()],
		["QUIT", func(): quit_pressed.emit()],
	]:
		var b := Parts.button(String(choice[0]), "Quiet")
		b.pressed.connect(choice[1])
		buttons.add_child(b)

	# The build, bottom left. A game held to a release standard needs a bug report
	# to be able to say which build it is.
	var footer := Parts.say(
		"%s  ·  Godot %s" % [_build_line(), Engine.get_version_info()["string"]], "Faint"
	)
	footer.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	footer.position = Vector2(96, -34)
	add_child(footer)


func _build_line() -> String:
	# Asked of the loaded simulation binary rather than read from a setting, and
	# that is the point: the only version number typed anywhere in this repository
	# is `workspace.package.version` in the root Cargo.toml, and this reports the
	# version of the DLL actually in memory. A second copy in project.godot would
	# be a number that could disagree with the binary a player is running, which is
	# worse than no number at all.
	return "Red Republic %s" % Build.version()


## Say whether there is a republic in progress, and describe it.
##
## The description comes from the save's own preview, so the menu cannot advertise
## a republic that is not there.
func set_continuable(available: bool, description: String) -> void:
	if _continue_button == null:
		return
	_continue_button.disabled = not available
	_continue_note.text = description if available else "no republic in progress"
	_continue_note.theme_type_variation = "Small" if available else "Faint"
