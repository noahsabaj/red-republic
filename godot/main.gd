extends Node3D

## Wires the scene to the simulation, and decides which screen is up.
##
## Everything this script does is one of four things: hand Godot a mesh built from
## simulation state, hand the camera the map's size, read a handful of numbers for
## the status line, or move between screens. It holds no simulation of its own, and
## it must not start: if a value here would change how a republic turns out, it
## belongs in crates/sim.
##
## The heavy transfers are one-off or event-driven, which is what makes the
## marshalling boundary affordable. The terrain mesh is built once when a republic
## arrives (30 ms, measured). Buildings are uploaded only when the count changes.
## Vehicles are the one per-frame read, and it is 0.9 microseconds.
##
## # One scene, one Republic, screens as layers
##
## The menu, the founding shelf, the settings and the game are all this scene with
## different `CanvasLayer`s visible. The alternative -- a scene per screen with
## `change_scene_to_file` -- would put the `Republic` node in whichever scene was
## loaded, and the founding screen's whole job is to hand a world to the thing that
## will play it. Two Republics means the world is generated in one and needed in
## the other.
##
## # The command-line checks still boot straight into a republic
##
## `--shot`, `--bench` and `--advance` exist as permanent checks and predate the
## menu; three bugs got past every number and were caught only by looking at a
## rendered frame. So any of those flags skips the menu and founds the default
## posting, and `--screen` renders a named screen instead -- which is how the menu
## and the shelf get looked at too.

const Looks := preload("res://looks.gd")
const Kit := preload("res://building_kit.gd")
const Art := preload("res://building_art.gd")
const Overlays := preload("res://ui/overlays.gd")
const Store := preload("res://settings_store.gd")
const SaveFiles := preload("res://saves.gd")

## The default posting, for a run that skips the founding screen.
##
## Only reachable from the command line now. A player chooses their land on the
## founding screen, and these are what a capture or a benchmark opens.
const SEED := 1961
const EXTENT_M := 6000.0
const CLIMATE := 0  ## indexes ClimateId::ALL: plains, taiga, steppe, maritime
## What a command-line posting is called. A real player names their own.
const DEFAULT_NAME := "Novo-Uralsk"
## How many settlers arrive is the simulation's to say, not this file's. It was
## a constant here once and the two drifted, which is how a founding ended up
## with more jobs than people and a customs house nobody ever worked.

## Which screen is up. The republic keeps running underneath PAUSE only if the
## player left it running -- see `_set_screen`.
enum Screen { MENU, FOUNDING, PLAYING, PAUSED, SETTINGS, SAVES, REFERENCE, RADIO }

@onready var republic: Node = $Republic
@onready var sounds: Node = $Sounds
@onready var rig: Node3D = $CameraRig
@onready var terrain_node: MeshInstance3D = $Terrain
@onready var buildings_node: MultiMeshInstance3D = $Buildings
@onready var vehicles_node: MultiMeshInstance3D = $Vehicles
@onready var newcomers_node: MultiMeshInstance3D = $Newcomers
@onready var roads_node: MeshInstance3D = $Roads
@onready var lines_node: MeshInstance3D = $Lines
@onready var ways_node: MeshInstance3D = $Ways
@onready var hud: CanvasLayer = $HUD
@onready var survey_node: MeshInstance3D = $Survey
@onready var frontier_node: MeshInstance3D = $Frontier
@onready var audio: Node = $Audio
@onready var menu: CanvasLayer = $Menu
@onready var founding: CanvasLayer = $Founding
@onready var settings: CanvasLayer = $Settings
@onready var saves: CanvasLayer = $Saves
@onready var reference: CanvasLayer = $Reference
@onready var radio: CanvasLayer = $Radio
@onready var pause_menu: CanvasLayer = $Pause

var _buildings_shown := -1
var _roads_shown := -1
var _lines_shown := -1
var _ways_shown := -1
var _shot_path := ""
var _shot_after := 90
var _frames := 0
var _start_speed := 0
var _bench_frames := 0
var _look: Looks.Look = null
var _view_distance := 0.0
var _kind_nodes: Array[MultiMeshInstance3D] = []
var _overlay := Overlays.Mode.NONE
var _start_overlay := ""
var _terrain_material: ShaderMaterial = null
var _overlay_dirty := true
var _overlay_day := -1
var _advance_days := 0
var _bench_times: PackedFloat64Array = PackedFloat64Array()
var _start_screen := ""
var _check_only := false

var _store: RefCounted = null
var _screen: int = Screen.MENU
## What the speed was before something paused the republic, so resuming puts it
## back rather than guessing. Speed is the republic's state; a screen only borrows
## it.
var _speed_before_pause := 1
## Which screen to return to when a stacked one closes. Settings and the reference
## open from both the menu and the pause overlay, and a hardcoded return would
## drop a paused player back to the main menu.
var _return_to: int = Screen.MENU
var _last_autosave_day := -1
var _extent := EXTENT_M
## Set once a republic exists, so the meshes are only built for a real world.
var _built := false


func _ready() -> void:
	_read_arguments()
	_look = Looks.current()
	_apply_look()

	_store = Store.new()
	_store.apply()
	_apply_interface_scale()
	audio.setup(sounds, _store)

	_connect_screens()

	# A capture or a benchmark run skips the menu: these flags are permanent
	# checks and existed before there was a menu to get in their way.
	var wants_republic := (_shot_path != "" or _bench_frames > 0
		or _advance_days > 0 or _check_only)
	if _start_screen != "" and _start_screen != "game":
		_open_named_screen(_start_screen)
	elif wants_republic or _start_screen == "game":
		_found_default()
	else:
		_set_screen(Screen.MENU)


## Found the constants above, for a run with no founding screen.
func _found_default() -> void:
	republic.found(SEED, EXTENT_M, CLIMATE, republic.founding_settlers())
	# Named, because an unnamed republic is a state nothing should be able to
	# observe and a capture run would otherwise be the one place it is. It also
	# means a screenshot of the HUD shows the name line doing its job rather than
	# the empty fallback.
	republic.rename(DEFAULT_NAME)
	_extent = EXTENT_M
	_enter_republic()
	if _advance_days > 0:
		republic.advance_days(_advance_days)
	if _view_distance > 0.0:
		rig.set_distance(_view_distance)
	match _start_overlay:
		"going": _overlay = Overlays.Mode.GOING
		"tracks": _overlay = Overlays.Mode.WEAR
		"survey": _overlay = Overlays.Mode.SURVEY
		"pollution": _overlay = Overlays.Mode.POLLUTION
		"snow": _overlay = Overlays.Mode.SNOW
	_set_screen(Screen.PLAYING)
	republic.set_speed(_start_speed)

	if OS.is_debug_build():
		# A headless run that errors prints; a headless run that quietly does
		# nothing also prints nothing. This is the line that tells the two
		# apart, and it is why it exists rather than trusting silence.
		var verts: int = terrain_node.mesh.surface_get_array_len(0)
		print("founded %s: %d verts, %d buildings, %d lorries, %d pop, %.0f m map" % [
			republic.date_text(), verts, republic.building_count(),
			republic.vehicle_count(), republic.population(), republic.map_extent(),
		])

	# Everything a load can break has now run: the scripts parsed, the extension
	# resolved, the art guard passed and a republic exists. Nothing further is
	# worth a CI job's time.
	if _check_only:
		# Which artifact is this? A packaged build is a genuinely different thing
		# from a development one -- optimised, no debug prints, vsync on -- and
		# every one of those differences is a project setting or a cargo profile
		# that can silently fail to apply. The packaging job asserts on this line,
		# so "the shipped build turns vsync back on" is a tested claim rather than
		# a comment in project.godot hoping somebody did it.
		#
		# `get_setting_with_override` and not `get_setting`: only the former
		# resolves the `.template_release` feature tag, and the latter would report
		# the development default from inside a correctly exported build. Read off
		# ProjectSettings rather than DisplayServer because this runs headless,
		# where there is no window to ask.
		print("build %s: vsync %s, debug %s" % [
			Build.semver(),
			ProjectSettings.get_setting_with_override("display/window/vsync/vsync_mode"),
			"yes" if OS.is_debug_build() else "no",
		])
		_check_saves()
		get_tree().quit()


## Write a save, list it, read it back, and check it is the same republic.
##
## The simulation's own round trip is tested in Rust and this is not that. What
## this covers is the part only the shell can get wrong: the write-to-a-temporary
## and rename, the preview marshalling, the listing, and the load path. A bug
## anywhere in there costs a player their republic, which makes it the highest
## stakes thing in this milestone and the one place a check is worth a CI job.
##
## Prints one line either way, because a check that quietly does nothing prints
## exactly as much as one that worked.
func _check_saves() -> void:
	var before := {
		"name": String(republic.republic_name()),
		"date": String(republic.date_text()),
		"population": republic.population(),
		"buildings": republic.building_count(),
	}

	var file_name: String = SaveFiles.name_for(before["name"], before["date"])
	var why: String = republic.save_to(SaveFiles.path_for(file_name))
	if why != "":
		printerr("save check FAILED to write: %s" % why)
		return

	# Through the listing rather than straight from the path, because the listing
	# is what the load screen actually shows and it is where the preview
	# marshalling is exercised.
	var listing: Array = SaveFiles.listing(republic)
	var found := {}
	for row in listing:
		if row["file"] == file_name:
			found = row
			break
	if found.is_empty():
		printerr("save check FAILED: %s was written and does not appear in the listing" % file_name)
		return
	if found["problem"] != "":
		printerr("save check FAILED: this build cannot read its own save: %s" % found["problem"])
		return
	if found["name"] != before["name"] or found["date"] != before["date"]:
		printerr("save check FAILED: listed as %s/%s, was %s/%s" % [
			found["name"], found["date"], before["name"], before["date"],
		])
		return

	why = republic.load_from(SaveFiles.path_for(file_name))
	if why != "":
		printerr("save check FAILED to load: %s" % why)
		return
	if String(republic.republic_name()) != before["name"] \
			or String(republic.date_text()) != before["date"] \
			or republic.population() != before["population"] \
			or republic.building_count() != before["buildings"]:
		printerr("save check FAILED: reloaded a different republic")
		return

	# Tidied up, so a CI run does not leave a save behind and a second run does
	# not find a stale one and pass on it.
	DirAccess.remove_absolute(SaveFiles.path_for(file_name))
	print("save check ok: %s, %s, %d people, %d buildings" % [
		before["name"], before["date"], before["population"], before["buildings"],
	])


## Everything that has to happen once a world exists, however it arrived.
##
## Founding and loading both come through here. They used to be the same code path
## only because there was one way in; now there are two, and a second copy of the
## mesh building is a second thing to forget to update.
func _enter_republic() -> void:
	_extent = republic.map_extent()
	_reset_shown()
	_build_terrain()
	if not _built:
		# The kit meshes are per building *kind*, not per world, so they survive a
		# reload. Rebuilding them would leak a MultiMeshInstance3D per kind on
		# every load.
		_build_instance_meshes()
		_built = true
	rig.frame_map(_extent, republic.centre_x(), republic.centre_y())
	hud.set_resource_names(republic.resource_names(), republic.resource_forms())
	hud.set_contentment_names(republic.contentment_names())
	hud.set_utility_names(republic.utility_names())
	hud.set_way_names(republic.way_names())
	hud.set_hint(
		"0-5 speed  ·  space pause  ·  esc menu  ·  F none  G going  T tracks  "
		+ "R survey  P smoke  N snow  ·  WASD pan  ·  right-drag orbit  ·  wheel zoom"
	)
	_refresh_buildings()
	_refresh_roads()
	_refresh_lines()
	_refresh_ways()
	_build_frontier()
	_overlay_dirty = true
	_last_autosave_day = -1
	republic.set_speed(0)


## Forget what was drawn, so a loaded republic does not inherit the last one's
## counts.
##
## Every one of these is an "only upload when it changes" cache, and a load is
## exactly the case where the count can come back *the same* while the contents
## are completely different -- two republics with sixteen buildings each. Without
## this the new world would render as the old one.
func _reset_shown() -> void:
	_buildings_shown = -1
	_roads_shown = -1
	_lines_shown = -1
	_ways_shown = -1
	_overlay_day = -1


func _connect_screens() -> void:
	menu.new_posting_pressed.connect(_on_new_posting)
	menu.continue_pressed.connect(_on_continue)
	menu.load_pressed.connect(func(): _open_saves(SaveFiles, Screen.MENU))
	menu.settings_pressed.connect(func(): _open_stacked(Screen.SETTINGS, Screen.MENU))
	menu.reference_pressed.connect(func(): _open_stacked(Screen.REFERENCE, Screen.MENU))
	menu.quit_pressed.connect(func(): get_tree().quit())

	founding.posting_taken.connect(_on_posting_taken)
	founding.cancelled.connect(func(): _set_screen(Screen.MENU))

	settings.closed.connect(func(): _set_screen(_return_to))
	reference.closed.connect(func(): _set_screen(_return_to))
	radio.closed.connect(func(): _set_screen(_return_to))
	saves.closed.connect(func(): _set_screen(_return_to))
	saves.loaded.connect(_on_loaded)

	pause_menu.resumed.connect(_on_resume)
	pause_menu.save_pressed.connect(func(): _open_saves_to_write())
	pause_menu.load_pressed.connect(func(): _open_saves(SaveFiles, Screen.PAUSED))
	pause_menu.settings_pressed.connect(func(): _open_stacked(Screen.SETTINGS, Screen.PAUSED))
	pause_menu.reference_pressed.connect(func(): _open_stacked(Screen.REFERENCE, Screen.PAUSED))
	pause_menu.radio_pressed.connect(func(): _open_stacked(Screen.RADIO, Screen.PAUSED))
	pause_menu.abandon_pressed.connect(_on_abandon)


func _on_new_posting() -> void:
	audio.play("click")
	_set_screen(Screen.FOUNDING)
	# The master seed comes from the clock, because a shelf is the one place a
	# player wants a different hand each time they open it. Everything downstream
	# of it is deterministic from that number, and the founding screen prints it.
	founding.open(republic, int(Time.get_unix_time_from_system()) & 0x7FFFFFFF)


func _on_posting_taken() -> void:
	audio.play("confirm")
	republic.close_shelf()
	_enter_republic()
	_set_screen(Screen.PLAYING)
	republic.set_speed(1)


## Open the most recent save.
##
## The newest by file time, which is what "continue" means to a player -- not the
## furthest along in game time, because a player who went back to an earlier save
## wants that one.
func _on_continue() -> void:
	var listing: Array = SaveFiles.listing(republic)
	for row in listing:
		if row["problem"] == "":
			if republic.load_from(row["path"]) == "":
				_on_loaded()
				return
	# Nothing openable. The button should have been disabled, so reaching here
	# means a save was removed since the menu was drawn; re-checking is the
	# honest response.
	audio.play("refuse")
	_refresh_continue()


func _on_loaded() -> void:
	audio.play("confirm")
	_enter_republic()
	_set_screen(Screen.PLAYING)


func _on_resume() -> void:
	audio.play("click")
	_set_screen(Screen.PLAYING)
	republic.set_speed(_speed_before_pause)


func _on_abandon() -> void:
	# The republic is not unloaded: there is no verb for that, and there does not
	# need to be. Taking a new posting or loading a save replaces the world, and
	# until then the menu simply does not show it.
	audio.play("click")
	_set_screen(Screen.MENU)


func _open_stacked(screen: int, back_to: int) -> void:
	audio.play("click")
	_return_to = back_to
	_set_screen(screen)


func _open_saves(_files, back_to: int) -> void:
	audio.play("click")
	_return_to = back_to
	_set_screen(Screen.SAVES)
	saves.open(republic, saves.Mode.LOAD)


func _open_saves_to_write() -> void:
	audio.play("click")
	_return_to = Screen.PAUSED
	_set_screen(Screen.SAVES)
	saves.open(republic, saves.Mode.SAVE)


## Show exactly one screen, and put the clock where that screen wants it.
##
## The clock is handled here rather than in each screen because there is one
## republic and one speed, and a screen that stopped time on its way in would have
## to be trusted to start it again on its way out.
func _set_screen(screen: int) -> void:
	_screen = screen
	menu.visible = screen == Screen.MENU
	founding.visible = screen == Screen.FOUNDING
	settings.visible = screen == Screen.SETTINGS
	saves.visible = screen == Screen.SAVES
	reference.visible = screen == Screen.REFERENCE
	radio.visible = screen == Screen.RADIO
	pause_menu.visible = screen == Screen.PAUSED
	hud.visible = screen == Screen.PLAYING

	if screen == Screen.PLAYING:
		return

	# Anything that is not the game stops the clock. Remembered once, so opening
	# settings from the pause overlay does not overwrite the speed the player was
	# actually running at with zero.
	if republic.speed() > 0:
		_speed_before_pause = republic.speed()
	republic.set_speed(0)

	match screen:
		Screen.MENU:
			audio.quieten()
			_refresh_continue()
		Screen.PAUSED:
			pause_menu.show_for(republic)
		Screen.SETTINGS:
			settings.open(_store, republic.is_founded())
		Screen.REFERENCE:
			reference.open(republic)
		Screen.RADIO:
			radio.open()


func _refresh_continue() -> void:
	var listing: Array = SaveFiles.listing(republic)
	for row in listing:
		if row["problem"] == "":
			menu.set_continuable(true, "%s  ·  %s  ·  %d people" % [
				row["name"], row["date"], row["population"],
			])
			return
	menu.set_continuable(false, "")


## Open a screen named on the command line, for capture runs.
##
## The reason this exists: three bugs in this scene got past every number and were
## caught only by looking at a rendered frame. Every screen M12 adds needs the same
## treatment, and it cannot get it unless a script can ask for one by name.
func _open_named_screen(name: String) -> void:
	match name:
		"menu":
			_set_screen(Screen.MENU)
		"founding":
			_set_screen(Screen.FOUNDING)
			founding.open(republic, SEED)
		"settings":
			_open_stacked(Screen.SETTINGS, Screen.MENU)
		"reference":
			_open_stacked(Screen.REFERENCE, Screen.MENU)
		"radio":
			_open_stacked(Screen.RADIO, Screen.MENU)
		"saves":
			_open_saves(SaveFiles, Screen.MENU)
		"pause":
			# A pause overlay over a real republic, which is the only way it is
			# ever seen.
			_found_default()
			_set_screen(Screen.PAUSED)
		_:
			push_error("no screen called '%s'" % name)
			_set_screen(Screen.MENU)


func _apply_interface_scale() -> void:
	# Applied to the whole viewport rather than per control, so one number moves
	# every screen and nothing can be missed.
	var scale: float = _store.interface_scale()
	if not is_equal_approx(scale, 1.0):
		get_window().content_scale_factor = scale


func _process(delta: float) -> void:
	if _screen == Screen.PLAYING:
		_refresh_buildings()
		_refresh_roads()
		_refresh_lines()
		_refresh_ways()
		_refresh_vehicles()
		_refresh_newcomers()
		_refresh_overlay()
		_refresh_status()
		_follow_the_day()
	_maybe_bench(delta)
	_maybe_capture()


## Things that happen once a simulated day has turned.
##
## The audio beds and the autosave both want a day boundary, and both read the
## date. One place watching the clock, because two would be two chances to watch
## it differently.
func _follow_the_day() -> void:
	if not audio.day_changed(republic.date_text()):
		return
	audio.follow(republic)
	_maybe_autosave()


func _maybe_autosave() -> void:
	var interval: int = _store.autosave_interval_days()
	if interval <= 0 or not republic.is_founded():
		return
	# Day index from the save preview would be a round trip through the disk; the
	# journal length would not move on a quiet day. The date string is what is to
	# hand, so days are counted from how many times this has been reached.
	var day := _day_number()
	if _last_autosave_day < 0:
		_last_autosave_day = day
		return
	if day - _last_autosave_day < interval:
		return
	_last_autosave_day = day
	var file_name: String = SaveFiles.autosave_name(String(republic.republic_name()))
	var why: String = republic.save_to(SaveFiles.path_for(file_name))
	if why != "":
		push_warning("autosave failed: %s" % why)


## A monotonically increasing day count, from the in-game date.
##
## Parsed from the date string rather than exposed as a number, because the shell
## already reads the date for the overlays and adding a second view for the same
## fact is the sort of duplication that drifts. Years are 360 days of 12 months,
## which is the simulation's calendar.
func _day_number() -> int:
	var parts: PackedStringArray = String(republic.date_text()).split("-")
	if parts.size() != 3:
		return 0
	return int(parts[0]) * 360 + (int(parts[1]) - 1) * 30 + int(parts[2])


## Time frames with the simulation genuinely running, then report and quit.
##
## Vsync is off in project.godot, and that is load-bearing: with it on every
## p50 comes back as exactly 16.67 ms whatever the load, so a scene that is
## drowning reports a healthy number. Measured on this machine -- it silently
## invalidated a whole probe run before anyone noticed.
##
## Real-time is the thesis, so "does it hold frame rate while the republic is
## actually being simulated" is not a nice-to-have measurement. It is the
## constraint the renderer exists under.
func _maybe_bench(delta: float) -> void:
	if _bench_frames <= 0:
		return
	_bench_times.append(delta * 1000.0)
	if _bench_times.size() < _bench_frames:
		return
	var sorted := _bench_times.duplicate()
	sorted.sort()
	# Drop the first tenth: pipeline compilation makes the opening frames
	# unrepresentative, and the first frame alone was 146 ms when measured.
	var warm := _bench_frames / 10
	var p50 := sorted[int(warm + (sorted.size() - warm) * 0.50)]
	var p95 := sorted[int(warm + (sorted.size() - warm) * 0.95)]
	print("bench speed %d: %d frames, p50 %.2f ms, p95 %.2f ms, %d buildings, %d lorries, %s" % [
		republic.speed(), _bench_frames, p50, p95,
		republic.building_count(), republic.vehicle_count(), republic.date_text(),
	])
	get_tree().quit()


## Capture the viewport and quit, when asked for on the command line:
##
##     godot --path godot -- --shot out.png [--after 90] [--speed 2] [--screen menu]
##
## In-process rather than an OS screenshot, so it captures exactly what was
## rendered with no window-focus or monitor guesswork, and it works the same
## from a script as from a hand. A rendered frame is the only way to check the
## things numbers cannot say -- that the terrain is the right way up, that
## buildings are standing on it rather than buried in it, that the camera is
## pointed at the republic, that a panel is not off the edge of the screen.
##
## **Never add `--headless` to a shot run.** The await below never returns with no
## rendering driver, so the process spins at flat memory for ever and prints
## nothing, which looks exactly like a simulation hang.
func _maybe_capture() -> void:
	if _shot_path == "":
		return
	_frames += 1
	if _frames < _shot_after:
		return
	await RenderingServer.frame_post_draw
	var image := get_viewport().get_texture().get_image()
	image.save_png(_shot_path)
	print("captured %s after %d frames" % [_shot_path, _frames])
	get_tree().quit()


func _unhandled_input(event: InputEvent) -> void:
	if not (event is InputEventKey and event.pressed and not event.echo):
		return

	# Escape is the one key that means something on every screen, and what it
	# means depends on where you are: out of the game into the pause overlay, and
	# out of a stacked screen back to whatever opened it.
	if event.keycode == KEY_ESCAPE:
		match _screen:
			Screen.PLAYING:
				_set_screen(Screen.PAUSED)
			Screen.PAUSED:
				_on_resume()
			Screen.MENU:
				pass
			Screen.FOUNDING:
				_set_screen(Screen.MENU)
			_:
				_set_screen(_return_to)
		return

	if _screen != Screen.PLAYING:
		return

	# 0 pauses; 1 is real-time -- one real second is one in-game second --
	# then a second buys 1, 2, 4 or 8 in-game hours.
	var speeds := {
		KEY_0: 0, KEY_1: 1, KEY_2: 2, KEY_3: 3, KEY_4: 4, KEY_5: 5,
	}
	if speeds.has(event.keycode):
		republic.set_speed(speeds[event.keycode])
	elif event.keycode == KEY_SPACE:
		republic.set_speed(0 if republic.speed() > 0 else 1)
	else:
		var modes := {
			KEY_F: Overlays.Mode.NONE,
			KEY_G: Overlays.Mode.GOING,
			KEY_T: Overlays.Mode.WEAR,
			KEY_R: Overlays.Mode.SURVEY,
			KEY_P: Overlays.Mode.POLLUTION,
			KEY_N: Overlays.Mode.SNOW,
		}
		if modes.has(event.keycode):
			_overlay = modes[event.keycode]
			_overlay_dirty = true


## Pause when the window loses focus, if the player asked for it.
##
## Off by default would be wrong for a game whose first speed is real time: a
## republic left running while its player answered an email has had an hour of
## unattended winter.
func _notification(what: int) -> void:
	if what != NOTIFICATION_APPLICATION_FOCUS_OUT:
		return
	if _screen != Screen.PLAYING or _store == null:
		return
	if bool(_store.get_value("play/pause_on_focus_loss")):
		_set_screen(Screen.PAUSED)


## Arguments after a bare `--` on the command line. Used for capture runs and
## for starting at a speed, so a check can be scripted without editing this file.
func _read_arguments() -> void:
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		match args[i]:
			"--shot":
				if i + 1 < args.size():
					_shot_path = args[i + 1]
			"--after":
				if i + 1 < args.size():
					_shot_after = int(args[i + 1])
			"--speed":
				if i + 1 < args.size():
					_start_speed = int(args[i + 1])
			"--bench":
				if i + 1 < args.size():
					_bench_frames = int(args[i + 1])
			"--dist":
				if i + 1 < args.size():
					_view_distance = float(args[i + 1])
			"--advance":
				if i + 1 < args.size():
					_advance_days = int(args[i + 1])
			"--overlay":
				if i + 1 < args.size():
					_start_overlay = args[i + 1]
			"--screen":
				if i + 1 < args.size():
					_start_screen = args[i + 1]
			"--check":
				# Found a republic, print the line that says so, and quit. What CI
				# runs, and it exists because the alternatives are both bad: a
				# `--shot` run needs a rendering driver and hangs for ever under
				# `--headless`, and a plain `--advance` run never exits, so the job
				# would depend on a timeout and report a failure for succeeding.
				_check_only = true


## Sun, sky and air. Presentation only -- the weather the simulation models is a
## different thing entirely, and this does not read it.
func _apply_look() -> void:
	var sun: DirectionalLight3D = $Sun
	sun.light_color = _look.sun_colour
	sun.light_energy = _look.sun_energy
	sun.rotation_degrees = Vector3(-_look.sun_elevation, _look.sun_azimuth, 0.0)
	sun.shadow_enabled = true

	var sky_mat := ProceduralSkyMaterial.new()
	sky_mat.sky_top_color = _look.sky_top
	sky_mat.sky_horizon_color = _look.sky_horizon
	sky_mat.ground_bottom_color = _look.ground_horizon
	sky_mat.ground_horizon_color = _look.ground_horizon
	sky_mat.sun_angle_max = 12.0

	var sky := Sky.new()
	sky.sky_material = sky_mat

	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = _look.ambient_colour
	env.ambient_light_energy = _look.ambient_energy
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	env.tonemap_exposure = _look.tonemap_exposure
	# Distance fog rather than volumetric: the job is to give a 6 km map depth,
	# not to render weather, and it costs nothing.
	env.fog_enabled = true
	env.fog_light_color = _look.fog_colour
	env.fog_density = _look.fog_density
	# The sky is already the right colour; letting fog repaint it flattens the
	# horizon into a single wash and takes the depth cue with it.
	env.fog_sky_affect = 0.0
	env.fog_aerial_perspective = 0.35
	env.ssao_enabled = true
	env.ssao_intensity = 1.2

	$Sky.environment = env


func _build_terrain() -> void:
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, republic.terrain_surface())
	terrain_node.mesh = mesh

	# Vertex colour carries the surface kind as a one-hot channel -- red grass,
	# green forest, blue rock, black water -- and terrain.gdshader decides what
	# each looks like. Keeping the tones there rather than in Rust is what lets
	# the art direction change without recompiling the simulation's renderer.
	var mat := ShaderMaterial.new()
	mat.shader = load("res://terrain.gdshader")
	mat.set_shader_parameter("grass_colour", _look.grass)
	mat.set_shader_parameter("forest_colour", _look.forest)
	mat.set_shader_parameter("rock_colour", _look.rock)
	mat.set_shader_parameter("water_colour", _look.water)
	mat.set_shader_parameter("contour_strength", _look.contour_strength)
	_terrain_material = mat
	terrain_node.material_override = mat


## One mesh per building kind, assembled from the kit, each with its own
## MultiMesh. Done once at load: a kind's mesh never changes, only how many of
## them are standing.
func _build_instance_meshes() -> void:
	var names := PackedStringArray()
	for k in republic.building_kind_count():
		names.append(republic.building_kind_name(k))
	# An unauthored building would otherwise render as a default box nobody
	# chose. This is the guard that makes adding a building to the Rust table
	# fail loudly here instead.
	Art.check(names)

	var parts := Kit.components()
	if OS.is_debug_build():
		var found := parts.keys()
		found.sort()
		print("kit components: %s" % ", ".join(found))
	var art := Art.table()
	for k in republic.building_kind_count():
		var size: Vector2 = republic.building_kind_size(k)
		var mesh := Kit.assemble(parts, art[names[k]], size.x, size.y, _look.tones)
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = mesh
		var node := MultiMeshInstance3D.new()
		node.multimesh = mm
		node.name = "Kind%d" % k
		buildings_node.add_child(node)
		_kind_nodes.append(node)

	var van := BoxMesh.new()
	van.size = Vector3(6.0, 3.0, 2.5)
	var van_mat := StandardMaterial3D.new()
	van_mat.albedo_color = _look.vehicle
	van.material = van_mat

	var vm := MultiMesh.new()
	vm.transform_format = MultiMesh.TRANSFORM_3D
	vm.mesh = van
	vehicles_node.multimesh = vm

	# Settlers standing at a frontier post. A marker rather than figures: what
	# matters is that they are somewhere on the map that a coach has to reach,
	# and a republic with no road out to that post can see the problem it has.
	var marker := CylinderMesh.new()
	# A marker post rather than a pile of people: tall and thin, so it reads
	# as a vertical mark at map zoom where a group-sized object would be a few
	# pixels indistinguishable from a shed. Verified by looking -- at physical
	# size it vanished into the terrain at the zoom a player actually watches
	# their whole republic from.
	marker.top_radius = 5.0
	marker.bottom_radius = 14.0
	marker.height = 70.0
	var marker_mat := StandardMaterial3D.new()
	marker_mat.albedo_color = Color(0.86, 0.62, 0.34)
	marker_mat.emission_enabled = true
	marker_mat.emission = Color(0.5, 0.28, 0.1)
	marker_mat.emission_energy_multiplier = 0.4
	marker.material = marker_mat

	var nm := MultiMesh.new()
	nm.transform_format = MultiMesh.TRANSFORM_3D
	nm.mesh = marker
	newcomers_node.multimesh = nm


func _refresh_buildings() -> void:
	# Event-driven rather than per-frame: a kind's transform buffer only changes
	# when something of that kind is commissioned or demolished, and uploading
	# it every frame would be paying the one cost this boundary is careful about
	# for nothing.
	var total: int = republic.building_count()
	if total == _buildings_shown:
		return
	_buildings_shown = total
	for k in _kind_nodes.size():
		var count: int = republic.building_count_of_kind(k)
		var mm: MultiMesh = _kind_nodes[k].multimesh
		if mm.instance_count != count:
			mm.instance_count = count
		if count > 0:
			mm.buffer = republic.building_transforms_of_kind(k)


func _refresh_vehicles() -> void:
	var flat: PackedFloat32Array = republic.vehicle_positions()
	var count := flat.size() / 3
	var mm := vehicles_node.multimesh
	if mm.instance_count != count:
		mm.instance_count = count
	for i in count:
		var at := Vector3(flat[i * 3], flat[i * 3 + 1] + 1.5, flat[i * 3 + 2])
		mm.set_instance_transform(i, Transform3D(Basis.IDENTITY, at))


## Settlers standing at the frontier, waiting for a coach.
##
## They have to be on the map. An immigrant who appeared inside an apartment
## block would be exactly the click-a-button-and-it-happens shape this build
## refuses -- and a group standing at a post with no road to it is a decision
## the player can act on, but only if they can see it.
##
## The marker grows with the size of the group, so a crowd reads as a crowd.
func _refresh_newcomers() -> void:
	var flat: PackedFloat32Array = republic.newcomers()
	var stride := 5
	var count := flat.size() / stride
	var mm := newcomers_node.multimesh
	if mm.instance_count != count:
		mm.instance_count = count
	for i in count:
		var x := flat[i * stride]
		var z := flat[i * stride + 1]
		var heads := flat[i * stride + 2]
		# Visitors stand a little lower than settlers, which is the only thing
		# separating them here on purpose: from six hundred metres up they are
		# the same fact -- a crowd at a post waiting for a coach -- and they draw
		# on the same pool of coaches, which is the decision they share.
		var visiting := flat[i * stride + 4] > 0.5
		var scale := clampf(0.6 + heads / 40.0, 0.6, 2.0) * (0.75 if visiting else 1.0)
		var at := Vector3(x, republic.ground_height(x, z) + 35.0 * scale, z)
		mm.set_instance_transform(
			i, Transform3D(Basis.IDENTITY.scaled(Vector3.ONE * scale), at)
		)


## The frontier, drawn once: a coloured band around the whole perimeter with a
## marker at each post.
##
## Built when a republic arrives and never rebuilt, because a frontier does not
## move. The two blocs hold different stretches and the colours are the only thing
## that says which way is west -- which decides what currency this republic can
## earn, so it is not decoration.
func _build_frontier() -> void:
	var line: PackedFloat32Array = republic.frontier_line(240)
	var stride := 4
	var count := line.size() / stride
	if count < 2:
		return
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	# Inward, so the band lies on the republic's own ground rather than off the
	# edge of the mesh where there is nothing to draw on.
	var mid := _extent * 0.5
	for i in count - 1:
		var a := Vector3(line[i * stride], 0.0, line[i * stride + 1])
		var b := Vector3(line[(i + 1) * stride], 0.0, line[(i + 1) * stride + 1])
		var bloc := int(line[i * stride + 2])
		var tone: Color = _look.bloc_east if bloc == 0 else _look.bloc_west
		a.y = republic.ground_height(a.x, a.z) + 1.0
		b.y = republic.ground_height(b.x, b.z) + 1.0
		var inward_a := (Vector3(mid, a.y, mid) - a).normalized() * 26.0
		var inward_b := (Vector3(mid, b.y, mid) - b).normalized() * 26.0
		var p0 := a
		var p1 := b
		var p2 := b + inward_b
		var p3 := a + inward_a
		for v in [p0, p3, p2, p0, p2, p1]:
			st.set_color(tone)
			st.add_vertex(v)

	# A post at each crossing, standing proud so it reads from altitude.
	var posts: PackedFloat32Array = republic.crossings()
	for i in posts.size() / 4:
		var px := posts[i * 4]
		var pz := posts[i * 4 + 1]
		var tone: Color = _look.bloc_east if int(posts[i * 4 + 2]) == 0 else _look.bloc_west
		var base := Vector3(px, republic.ground_height(px, pz), pz)
		_pillar(st, base, 9.0, 26.0, tone)

	st.generate_normals()
	var mesh: ArrayMesh = st.commit()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	mesh.surface_set_material(0, mat)
	frontier_node.mesh = mesh


func _pillar(st: SurfaceTool, base: Vector3, half: float, height: float, tone: Color) -> void:
	var corners := [
		Vector3(-half, 0.0, -half), Vector3(half, 0.0, -half),
		Vector3(half, 0.0, half), Vector3(-half, 0.0, half),
	]
	for i in 4:
		var a: Vector3 = base + corners[i]
		var b: Vector3 = base + corners[(i + 1) % 4]
		var at := a + Vector3(0.0, height, 0.0)
		var bt := b + Vector3(0.0, height, 0.0)
		for v in [a, b, bt, a, bt, at]:
			st.set_color(tone)
			st.add_vertex(v)


func _refresh_roads() -> void:
	# Roads appear without being ordered -- traffic wears the ground, and a worn
	# corridor is promoted into the network as a dirt track -- so the segment
	# count genuinely changes while nobody is building.
	#
	# Drawn as ribbons rather than lines. A hairline reads as debug geometry,
	# and it also cannot show a grade: a dirt track and tarmac differ in width
	# and colour here because they differ in what a lorry can do on them, and
	# that has to be visible without opening a panel.
	var flat: PackedFloat32Array = republic.road_segments()
	var stride := 7
	var count := flat.size() / stride
	if count == _roads_shown:
		return
	_roads_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		mat.roughness = 0.9
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
		for i in count:
			var o := i * stride
			var a := Vector3(flat[o], flat[o + 1], flat[o + 2])
			var b := Vector3(flat[o + 3], flat[o + 4], flat[o + 5])
			var kph := flat[o + 6]
			var along := b - a
			along.y = 0.0
			if along.length() < 0.01:
				continue
			# Dirt 25, gravel 45, paved 60 km/h. Width and tone follow from the
			# limit, so a grade added later needs nothing here.
			var t := clampf((kph - 25.0) / 35.0, 0.0, 1.0)
			var half := lerpf(2.6, 4.4, t)
			var tone := _look.road_dirt.lerp(_look.road_paved, t)
			var side := along.normalized().cross(Vector3.UP).normalized() * half
			# Lifted clear of the ground so it does not z-fight the terrain.
			var lift := Vector3(0.0, 0.35, 0.0)
			var p0 := a - side + lift
			var p1 := a + side + lift
			var p2 := b + side + lift
			var p3 := b - side + lift
			# Wound so the ribbon faces UP. Getting this backwards is the second
			# time in this scene that correct-looking geometry rendered as
			# nothing at all -- the terrain did it first. Godot culls back
			# faces, and a flat quad seen from the wrong side is invisible
			# rather than wrong-looking, which is what makes it so easy to miss.
			for v in [p0, p3, p2, p0, p2, p1]:
				mesh.surface_set_color(tone)
				mesh.surface_add_vertex(v)
		mesh.surface_end()
	roads_node.mesh = mesh


## Power lines and heat mains, drawn as thin ribbons above the ground.
##
## They have to be on the map. A plant lights only what it is strung to and a
## boiler warms only what a main runs past, so "why is this block cold" is a
## question about a line the player either drew or did not -- which is
## unanswerable if the lines are invisible.
##
## Event-driven like the roads: a span changes only when one is ordered or
## energised, which is rare.
func _refresh_lines() -> void:
	var built: PackedFloat32Array = republic.utility_lines()
	var sites: PackedFloat32Array = republic.utility_sites()
	@warning_ignore("integer_division")
	var count: int = built.size() / 5 + sites.size() / 6
	if count == _lines_shown:
		return
	_lines_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.cull_mode = BaseMaterial3D.CULL_DISABLED
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
		@warning_ignore("integer_division")
		var spans: int = built.size() / 5
		for i in spans:
			var o := i * 5
			var tone: Color = _utility_tone(int(built[o + 4]))
			_span(mesh, built[o], built[o + 1], built[o + 2], built[o + 3], tone, 1.8, 9.0)
		@warning_ignore("integer_division")
		var pending: int = sites.size() / 6
		for i in pending:
			var o := i * 6
			# A site is thinner and paler, and fills in as it is strung -- the
			# same rule a road site answers to: a half-built thing must look
			# half-built rather than leave the player wondering why the lights
			# are still out.
			var tone: Color = _utility_tone(int(sites[o + 5]))
			tone.a = lerpf(0.25, 0.8, sites[o + 4])
			_span(mesh, sites[o], sites[o + 1], sites[o + 2], sites[o + 3], tone, 1.0, 5.0)
		mesh.surface_end()
	lines_node.mesh = mesh


## The rails and the rivers, drawn from the same span machinery as the grid.
##
## Roads have their own mesh already; these are the three ways that did not
## exist before. Water is in here beside the built ones deliberately -- the one
## network nobody builds is invisible on the ground at this camera height, and a
## republic that cannot see its river will not think to put a port on it.
##
## Air is deliberately **not** drawn. It is a fully connected graph between
## aerodromes, so drawing it means a spray of straight lines across the whole
## map that says nothing a list of aerodromes does not; the HUD carries its
## length instead.
func _refresh_ways() -> void:
	var rail: PackedFloat32Array = republic.ways(WAY_RAIL)
	var water: PackedFloat32Array = republic.ways(WAY_WATER)
	@warning_ignore("integer_division")
	var count: int = (rail.size() + water.size()
		+ republic.ways(WAY_TRAM).size() + republic.ways(WAY_METRO).size()) / 5
	if count == _ways_shown:
		return
	_ways_shown = count
	var mesh := ImmediateMesh.new()
	if count > 0:
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.cull_mode = BaseMaterial3D.CULL_DISABLED
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
		# Lift matters more than it looks. Water was first drawn 0.4 m off the
		# ground, which reads as a hatched mess at map height rather than as a
		# river: a flat ribbon that close to the terrain z-fights with it, and
		# the result looks like a rendering fault rather than like a feature.
		# Caught by looking at a frame -- nothing that counts spans can see it.
		for pair in [
			[rail, RAIL_TONE, 6.0, 2.5],
			[republic.ways(WAY_TRAM), TRAM_TONE, 4.0, 2.0],
			[republic.ways(WAY_METRO), METRO_TONE, 5.0, 0.8],
			[water, WATER_TONE, 30.0, 1.5],
		]:
			var spans: PackedFloat32Array = pair[0]
			@warning_ignore("integer_division")
			var n: int = spans.size() / 5
			for i in n:
				var o := i * 5
				_span(mesh, spans[o], spans[o + 1], spans[o + 2], spans[o + 3],
					pair[1], pair[2], pair[3])
		mesh.surface_end()
	ways_node.mesh = mesh


## Indices into `Medium::ALL`, which is the order the shell hands them over in.
const WAY_ROAD := 0
const WAY_RAIL := 1
const WAY_TRAM := 2
const WAY_METRO := 3
const WAY_WATER := 4
const WAY_AIR := 5

## Track reads as dark steel; water as a broad pale ribbon lying almost flat on
## the ground, because it is the surface rather than something built on it.
const RAIL_TONE := Color(0.32, 0.30, 0.28)
const WATER_TONE := Color(0.35, 0.47, 0.60, 0.85)
## Street track is lighter than mainline rail and thinner; the metro is drawn
## faintly and low, because it is under the ground and the player is being
## shown where it runs rather than a thing standing on the map.
const TRAM_TONE := Color(0.52, 0.48, 0.44)
const METRO_TONE := Color(0.46, 0.38, 0.52, 0.55)


## In the order `Utility::ALL` declares: power, heat, conveyor, pipeline.
##
## Four distinct hues rather than a ramp, because these are categories and not a
## quantity -- a player has to be able to tell a belt from a pipe at a glance,
## and two shades of the same colour would read as "more of the same thing".
const UTILITY_TONES := [
	Color(0.86, 0.80, 0.42),  # power: overhead line
	Color(0.78, 0.42, 0.32),  # heat: hot main
	Color(0.55, 0.60, 0.66),  # conveyor: steel belt
	Color(0.44, 0.56, 0.44),  # pipeline: painted pipe
]


func _utility_tone(kind: int) -> Color:
	return UTILITY_TONES[kind] if kind >= 0 and kind < UTILITY_TONES.size() else Color.WHITE


## One ribbon between two ground positions, lifted clear and wound to face up.
##
## Wound so the ribbon faces UP. Getting this backwards is the third time in
## this scene that correct-looking geometry would render as nothing at all --
## Godot culls back faces, and a flat quad seen from the wrong side is invisible
## rather than wrong-looking.
func _span(
	mesh: ImmediateMesh, ax: float, az: float, bx: float, bz: float,
	tone: Color, half: float, lift: float
) -> void:
	var a := Vector3(ax, republic.ground_height(ax, az) + lift, az)
	var b := Vector3(bx, republic.ground_height(bx, bz) + lift, bz)
	var along := b - a
	along.y = 0.0
	if along.length() < 0.01:
		return
	var side := along.normalized().cross(Vector3.UP).normalized() * half
	var p0 := a - side
	var p1 := a + side
	var p2 := b + side
	var p3 := b - side
	for v in [p0, p3, p2, p0, p2, p1]:
		mesh.surface_set_color(tone)
		mesh.surface_add_vertex(v)


const SPEED_NAMES := ["paused", "real time", "1 h/s", "2 h/s", "4 h/s", "8 h/s"]


func _refresh_status() -> void:
	hud.refresh(republic, _overlay, SPEED_NAMES)


## Overlays are rebuilt on the day boundary rather than per frame.
##
## Going and wear are ground state and the ground changes daily, so a per-frame
## rebuild would repaint an identical texture sixty times a second. This is the
## same event-driven discipline the building buffer uses, for the same reason:
## the marshalling boundary is affordable precisely because nothing bulk crosses
## it on a schedule.
func _refresh_overlay() -> void:
	if _terrain_material == null:
		return
	var day: int = int(republic.date_text().replace("-", ""))
	if not _overlay_dirty and day == _overlay_day:
		return
	_overlay_dirty = false
	_overlay_day = day
	Overlays.apply(_terrain_material, republic, _overlay, _extent)
	if _overlay == Overlays.Mode.SURVEY:
		survey_node.mesh = Overlays.survey_mesh(republic, _ground_height)
	else:
		survey_node.mesh = null


func _ground_height(x: float, z: float) -> float:
	# The terrain mesh is the ground, so a disc drawn on it has to sit on the
	# same surface rather than on a plane at zero.
	return republic.ground_height(x, z)
