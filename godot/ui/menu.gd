extends CanvasLayer

## The main menu: the first thing a stranger sees.
##
## Built in code rather than authored as a `.tscn`, and that is a considered
## choice for every screen M12 adds. A hand-written scene file is a second
## language to get wrong with no compiler behind it, and the one thing this
## project has learned repeatedly about Godot is that a mistake in scene setup
## fails *silently* -- geometry wound the wrong way renders as nothing, a
## `class_name` needs an import pass, a node path typo is a null at runtime. Code
## gets parsed.
##
## The cost is that layout is not visual, which matters least here: these are
## columns of buttons and rows of labels, not a composed 3D scene.

const Style := preload("res://ui/theme.gd")

signal new_posting_pressed
signal continue_pressed
signal load_pressed
signal settings_pressed
signal reference_pressed
signal quit_pressed

var _continue_button: Button = null
var _continue_note: Label = null


func _ready() -> void:
	layer = 10
	add_child(Style.backdrop())

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 88)
	margin.add_theme_constant_override("margin_top", 72)
	margin.add_theme_constant_override("margin_bottom", 56)
	add_child(margin)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 10)
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	margin.add_child(column)

	# The title carries the fiction, and the subtitle carries the thesis. A
	# stranger should be able to read what kind of game this is off this screen
	# without being told -- there is no tutorial and no advisor to explain it
	# later.
	var title := Style.heading("RED REPUBLIC", Style.SIZE_TITLE, Style.INK)
	title.add_theme_constant_override("outline_size", 0)
	column.add_child(title)

	var subtitle := Style.body(
		"A planned economy, one second at a time.", Style.ACCENT_HOT
	)
	column.add_child(subtitle)

	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0, 34)
	column.add_child(spacer)

	var buttons := VBoxContainer.new()
	buttons.add_theme_constant_override("separation", 7)
	# SHRINK_BEGIN, or the column fills the window and the buttons run off the
	# right edge -- `custom_minimum_size` is a floor and not a width. Caught by
	# looking at a rendered frame; nothing else can see it.
	buttons.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	buttons.custom_minimum_size = Vector2(320, 0)
	column.add_child(buttons)

	# Continue sits first and is the primary action once there is something to
	# continue, because a returning player is the common case and a new posting
	# is the rare one. On a first run it is disabled with a note saying why,
	# rather than hidden -- a menu whose items move between runs is a menu a
	# player has to re-read.
	_continue_button = Style.button("Continue", true)
	_continue_button.pressed.connect(func(): continue_pressed.emit())
	buttons.add_child(_continue_button)

	_continue_note = Style.small("", Style.INK_FAINT)
	buttons.add_child(_continue_note)

	var new_button := Style.button("Take a new posting")
	new_button.pressed.connect(func(): new_posting_pressed.emit())
	buttons.add_child(new_button)

	var load_button := Style.button("Load a republic")
	load_button.pressed.connect(func(): load_pressed.emit())
	buttons.add_child(load_button)

	buttons.add_child(Style.divider())

	var reference_button := Style.button("Reference")
	reference_button.pressed.connect(func(): reference_pressed.emit())
	buttons.add_child(reference_button)

	var settings_button := Style.button("Settings")
	settings_button.pressed.connect(func(): settings_pressed.emit())
	buttons.add_child(settings_button)

	var quit_button := Style.button("Quit")
	quit_button.pressed.connect(func(): quit_pressed.emit())
	buttons.add_child(quit_button)

	# The version, bottom left, small. A build held to a release standard needs a
	# bug report to be able to say which build it is.
	var footer := Style.small(
		"%s  ·  Godot %s" % [_build_line(), Engine.get_version_info()["string"]],
		Style.INK_FAINT
	)
	footer.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	footer.position = Vector2(88, -32)
	add_child(footer)


func _build_line() -> String:
	# Asked of the loaded simulation binary rather than read from a setting, and
	# that is the point: the only version number typed anywhere in this
	# repository is `workspace.package.version` in the root Cargo.toml, and this
	# reports the version of the DLL actually in memory. A second copy in
	# project.godot would be a number that could disagree with the binary a
	# player is running, which is worse than no number at all.
	return "Red Republic %s" % Build.version()


## Say whether there is a republic in progress, and describe it.
##
## The description comes from the save's own preview, so the menu cannot advertise
## a republic that is not there.
func set_continuable(available: bool, description: String) -> void:
	if _continue_button == null:
		return
	_continue_button.disabled = not available
	if available:
		_continue_note.text = description
		_continue_note.add_theme_color_override("font_color", Style.INK_DIM)
	else:
		_continue_note.text = "no republic in progress"
		_continue_note.add_theme_color_override("font_color", Style.INK_FAINT)
