extends RefCounted

## Settings that survive the game closing.
##
## Written to `user://settings.cfg`, which on Windows is under `%APPDATA%`. Not
## into the project directory: a game installed by an installer has no business
## writing to its own install folder, and M13 puts this behind one.
##
## # Every setting has a default here and nowhere else
##
## The release standard says settings that work. The failure mode this file is
## shaped against is a setting whose default lives in the control that edits it,
## because then a fresh install and a reset button disagree -- and only one of
## them is ever tested. [`DEFAULTS`] is the single copy; the settings screen
## reads it for its own reset, and `apply` reads it for anything missing.
##
## Presentation and comfort only. **Nothing here may change how a republic turns
## out**, which is the same rule `looks.gd` answers to and the reason there is no
## difficulty entry: the land is the difficulty, and a knob that dialled it down
## would say the land choice never meant anything.

const PATH := "user://settings.cfg"

## Every setting, its section, and its default.
##
## Ordered as the settings screen shows them. `kind` drives which control is
## built, so adding a setting is a row here rather than a row here *and* a widget
## there -- the mistake the HUD's tables made four times over.
const DEFAULTS := [
	# Sound. Decibel-free: a share from 0 to 1 is what a player understands, and
	# `AudioServer` wants decibels, so the conversion happens in one place.
	{"key": "audio/master", "label": "Master volume", "section": "Sound", "kind": "share", "default": 0.8},
	{"key": "audio/ambience", "label": "Ambience", "section": "Sound", "kind": "share", "default": 0.7},
	{"key": "audio/machinery", "label": "Machinery", "section": "Sound", "kind": "share", "default": 0.6},
	{"key": "audio/interface", "label": "Interface", "section": "Sound", "kind": "share", "default": 0.5},
	{"key": "audio/radio", "label": "State Radio", "section": "Sound", "kind": "share", "default": 0.7},

	# Display. Vsync defaults ON here and OFF in project.godot, and the
	# difference is deliberate: the project file is the development default
	# because a benchmark under vsync reports 16.67 ms whatever the load, and a
	# player wants a frame that does not tear. This is the shipped default.
	{"key": "display/mode", "label": "Window", "section": "Display", "kind": "choice",
		"default": 0, "options": ["Windowed", "Full screen", "Borderless"]},
	{"key": "display/vsync", "label": "Vertical sync", "section": "Display", "kind": "switch", "default": true},
	{"key": "display/scale", "label": "Interface scale", "section": "Display", "kind": "choice",
		"default": 1, "options": ["Small", "Normal", "Large"]},

	# The republic. Comfort, not difficulty -- read the module note above.
	{"key": "play/autosave_days", "label": "Autosave every", "section": "The Republic", "kind": "choice",
		"default": 2, "options": ["Never", "10 days", "30 days", "90 days"]},
	{"key": "play/pause_on_focus_loss", "label": "Pause when the window loses focus",
		"section": "The Republic", "kind": "switch", "default": true},
	{"key": "play/confirm_demolish", "label": "Confirm before demolishing",
		"section": "The Republic", "kind": "switch", "default": false},
]

## How many simulated days each autosave option means. Index matches the
## `play/autosave_days` options above; zero is never.
const AUTOSAVE_DAYS := [0, 10, 30, 90]

## Interface scale factors, matching `display/scale`.
const SCALES := [0.85, 1.0, 1.2]

var _values := {}


func _init() -> void:
	for row in DEFAULTS:
		_values[row["key"]] = row["default"]
	load_from_disk()


func get_value(key: String):
	return _values.get(key, _default_for(key))


func set_value(key: String, value) -> void:
	_values[key] = value


func _default_for(key: String):
	for row in DEFAULTS:
		if row["key"] == key:
			return row["default"]
	return null


## Put everything back to the values a fresh install would have.
func reset() -> void:
	for row in DEFAULTS:
		_values[row["key"]] = row["default"]


## Read stored values over the defaults.
##
## `path` exists so `--check` can round-trip through a scratch file instead of
## the player's own settings. Testing persistence against `PATH` would mean
## writing over a real configuration to prove writing works, which is a bad
## trade on the one machine where the settings are somebody's.
func load_from_disk(path: String = PATH) -> void:
	var file := ConfigFile.new()
	if file.load(path) != OK:
		# No settings file is the normal state on a first run, not an error. The
		# defaults are already in place.
		return
	for row in DEFAULTS:
		var parts: PackedStringArray = String(row["key"]).split("/")
		if parts.size() != 2:
			continue
		if file.has_section_key(parts[0], parts[1]):
			var stored = file.get_value(parts[0], parts[1])
			# Type-checked against the default rather than trusted. A settings
			# file is a text file a player can edit, and a string where a float
			# belongs would otherwise reach `AudioServer` or a window mode.
			if typeof(stored) == typeof(row["default"]):
				_values[row["key"]] = stored


func save_to_disk(path: String = PATH) -> void:
	var file := ConfigFile.new()
	for row in DEFAULTS:
		var parts: PackedStringArray = String(row["key"]).split("/")
		if parts.size() == 2:
			file.set_value(parts[0], parts[1], _values[row["key"]])
	file.save(path)


## Push the display and audio settings at the engine.
##
## Called after any change and once at startup, so there is exactly one path from
## a stored value to a live one. A screen that applied its own changes would be a
## second path, and the one that gets tested is never the one that runs at boot.
func apply() -> void:
	var mode: int = int(get_value("display/mode"))
	match mode:
		1: DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_EXCLUSIVE_FULLSCREEN)
		2: DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_FULLSCREEN)
		_: DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_WINDOWED)

	DisplayServer.window_set_vsync_mode(
		DisplayServer.VSYNC_ENABLED if bool(get_value("display/vsync"))
		else DisplayServer.VSYNC_DISABLED
	)

	for bus_name in ["Ambience", "Machinery", "Interface", "Radio"]:
		var index := AudioServer.get_bus_index(bus_name)
		if index >= 0:
			AudioServer.set_bus_volume_db(index, decibels(bus_share(bus_name)))
	AudioServer.set_bus_volume_db(0, decibels(float(get_value("audio/master"))))


func bus_share(bus_name: String) -> float:
	return float(get_value("audio/%s" % bus_name.to_lower()))


## A 0..1 share as decibels, with zero meaning silent rather than very quiet.
##
## `linear_to_db(0.0)` is negative infinity, which Godot accepts and which then
## poisons any arithmetic done on it later. -80 dB is inaudible and finite.
static func decibels(share: float) -> float:
	if share <= 0.0005:
		return -80.0
	return linear_to_db(clampf(share, 0.0, 1.0))


func autosave_interval_days() -> int:
	var index := int(get_value("play/autosave_days"))
	return AUTOSAVE_DAYS[clampi(index, 0, AUTOSAVE_DAYS.size() - 1)]


func interface_scale() -> float:
	var index := int(get_value("display/scale"))
	return SCALES[clampi(index, 0, SCALES.size() - 1)]
