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
const Forest := preload("res://forest.gd")
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
enum Screen {
	MENU, FOUNDING, PLAYING, PAUSED, SETTINGS, SAVES, REFERENCE, RADIO,
	TRADE, FINANCE, JOURNAL, YEAR,
}

## **Typed as `Republic`, not as `Node`, and that is a bug fix rather than
## tidiness.** gdext registers `Republic` as a real Godot class with typed method
## signatures, but a variable declared `Node` makes every call on it a dynamic
## one returning `Variant` — so `var x := republic.anything()` is a *parse error*,
## and a GDScript parse error makes `--check` hang for ever instead of failing.
## That cost three separate stalls in one day, twice at eight minutes, before
## anybody asked why the annotations were needed at all. Naming the class is what
## makes the whole failure unrepresentable: the compiler now knows what every
## binding returns.
@onready var republic: Republic = $Republic
@onready var sounds: Node = $Sounds
@onready var rig: Node3D = $CameraRig
@onready var terrain_node: MeshInstance3D = $Terrain
## Holds one MultiMeshInstance3D per tree species. A plain Node3D rather than a
## MultiMeshInstance3D itself, because there are three species and a MultiMesh
## carries exactly one mesh.
@onready var forest_node: Node3D = $Forest
@onready var buildings_node: MultiMeshInstance3D = $Buildings
@onready var vehicles_node: MultiMeshInstance3D = $Vehicles
@onready var newcomers_node: MultiMeshInstance3D = $Newcomers
@onready var roads_node: MeshInstance3D = $Roads
@onready var road_sites_node: MeshInstance3D = $RoadSites
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
@onready var build_menu: CanvasLayer = $Build
@onready var labour_screen: CanvasLayer = $Labour
## The building inspector. A panel over the map rather than a screen instead of
## it, so it lives beside the HUD rather than in the `Screen` enum.
@onready var inspector: CanvasLayer = $Inspector
@onready var vehicle_panel: CanvasLayer = $VehiclePanel
@onready var trade_screen: CanvasLayer = $Trade
@onready var finance_screen: CanvasLayer = $Finance
@onready var journal_screen: CanvasLayer = $Journal
@onready var year_screen: CanvasLayer = $Year

var _buildings_shown := -1
var _roads_shown := -1
var _sites_shown := -1
var _lines_shown := -1
var _ways_shown := -1
## Whether a capture run should stand the fixture town up first, so there is
## something in the frame to look at.
const VehicleArt := preload("res://vehicle_art.gd")

## Which body index each vehicle kind is drawn with, and the node per body.
var _vehicle_body_of: Array[int] = []
var _vehicle_nodes: Array[MultiMeshInstance3D] = []

var _fixture_town := false
var _shot_path := ""
var _shot_after := 90
var _frames := 0
var _start_speed := 0
var _bench_frames := 0
var _look: Looks.Look = null
var _view_distance := 0.0
## Degrees above horizontal for a capture run. Zero means leave the default 50.
var _view_pitch := 0.0
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
## What the build menu picked, or -1 for nothing. `_build_market` is -1 to build
## it with the republic's own crews, or a bloc index to contract it out.
var _build_kind := -1
var _build_market := -1
## The translucent box that follows the cursor while placing.
var _ghost: MeshInstance3D = null
## Laying a way: the grade in hand, whether it takes lamps, and where the first
## click landed. A road is two points rather than one, so it is its own mode
## beside the building ghost rather than a shape the ghost can take.
var _span_grade := -1
## Stringing a line: which kind, or -1. Held apart from the grade rather than
## folded into it, because the two end in different commands and a single
## number with two meanings is how one of them gets issued as the other.
var _span_line := -1
var _span_lamps := false
var _span_from := Vector3.ZERO
var _span_started := false
var _span_ghost: MeshInstance3D = null


func _ready() -> void:
	_read_arguments()
	_look = Looks.current()
	_apply_look()

	_store = Store.new()
	_store.apply()
	_apply_interface_scale()
	audio.setup(sounds, _store)

	_connect_screens()

	# **A capture is pinned to the project's own resolution.**
	#
	# `--shot` grabs `get_viewport()`, which follows the window, and the window
	# is whatever the desktop gives it — so captures came back 1600x900 on one
	# run and 1028x749 on the next with nothing in the project changed. That
	# makes the tool useless for the one thing it exists for: two frames of the
	# same screen are not comparable if they are not the same size, and a layout
	# read off a 1028-wide frame says nothing about the shipped one.
	#
	# It cost real work to find out. A column looked badly unbalanced in a narrow
	# capture, I traced the width to a long unwrapped label, shortened the copy on
	# that theory, and the next capture came back narrow again — with the change
	# reverted. The window had never been the label's doing.
	if _shot_path != "":
		var wanted := Vector2i(
			ProjectSettings.get_setting("display/window/size/viewport_width", 1600),
			ProjectSettings.get_setting("display/window/size/viewport_height", 900)
		)
		DisplayServer.window_set_size(wanted)

	# A capture or a benchmark run skips the menu: these flags are permanent
	# checks and existed before there was a menu to get in their way.
	var wants_republic := (_shot_path != "" or _bench_frames > 0
		or _advance_days > 0 or _check_only)
	if _start_screen != "" and _start_screen != "game":
		_open_named_screen(_start_screen)
	elif wants_republic or _start_screen == "game":
		_found_default()
		if _fixture_town:
			republic.found_fixture_town(143)
			# Two days, so the labour pass has run and the fleet has been
			# dispatched — a lorry parked in a depot is not a lorry on a road,
			# and the road is where anybody looks at one.
			republic.advance_days(2)
	else:
		_set_screen(Screen.MENU)


## The lorry body out of `models/vehicles.glb`, or a box if it is missing.
##
## The fallback is not defensive habit: an export that failed silently would
## otherwise take the whole fleet off the map, and a republic whose lorries have
## vanished looks exactly like a republic whose lorries are not being
## dispatched -- which is a bug this repository has actually had.
func _vehicle_mesh(body: String) -> Mesh:
	var packed: PackedScene = load("res://models/vehicles.glb")
	if packed != null:
		var root := packed.instantiate()
		var found := _first_mesh_named(root, body)
		root.queue_free()
		if found != null:
			return found
	push_warning("no %s body in models/vehicles.glb — drawing boxes" % body)
	var box := BoxMesh.new()
	box.size = Vector3(6.0, 3.0, 2.5)
	var mat := StandardMaterial3D.new()
	mat.albedo_color = _look.vehicle
	box.material = mat
	return box


func _first_mesh_named(node: Node, wanted: String) -> Mesh:
	if node is MeshInstance3D and node.name.begins_with(wanted):
		return (node as MeshInstance3D).mesh
	for child in node.get_children():
		var found := _first_mesh_named(child, wanted)
		if found != null:
			return found
	return null


## Point the camera where a capture run asked for, after everything that frames
## it has finished framing it.
func _apply_view_flags() -> void:
	if _view_at.x >= 0.0:
		rig.frame_map(_extent, _view_at.x, _view_at.y)
	if _view_distance > 0.0:
		rig.set_distance(_view_distance)
	if _view_pitch > 0.0:
		rig.set_pitch(_view_pitch)


## Found the constants above, for a run with no founding screen.
func _found_default() -> void:
	republic.found(SEED, EXTENT_M, CLIMATE)
	# Named, because an unnamed republic is a state nothing should be able to
	# observe and a capture run would otherwise be the one place it is. It also
	# means a screenshot of the HUD shows the name line doing its job rather than
	# the empty fallback.
	republic.rename(DEFAULT_NAME)
	_extent = EXTENT_M
	_enter_republic()
	if _advance_days > 0:
		republic.advance_days(_advance_days)
	if _start_hour >= 0.0:
		republic.advance_to_hour(_start_hour)
	_apply_view_flags()
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
		# from a development one -- a different cargo profile, different Godot
		# export templates, no debug prints -- and an export that silently produced
		# the development one would look exactly like success. The packaging job
		# asserts on this line, which is the only thing in the output that can tell
		# them apart.
		#
		# It deliberately reports nothing else. An earlier version printed the
		# resolved vsync project setting too, which was worse than useless: the
		# settings store applies its own vsync a few lines after this runs, so the
		# number named a value nothing downstream reads.
		print("build %s: %s" % [
			Build.semver(),
			"development" if OS.is_debug_build() else "release",
		])
		_check_saves()
		_check_settings()
		_check_building()
		_check_labour()
		_check_road()
		_check_lines()
		_check_reference()
		_check_inspector()
		_check_trade()
		_check_finance()
		_check_journal()
		# Last, because it is the one check that has to put a working town on the
		# map to have anything to look at: a freshly founded republic owns no
		# vehicles, and standing the fixture up changes the world every earlier
		# check reasons about.
		_check_vehicle()
		get_tree().quit()


## Compile the reference and check every authored roster reaches it.
##
## **This is the guard that came across with the reference itself.** The document
## used to be composed in Rust, where a unit test asserted that every building,
## good, vehicle and grade appeared in it -- because there is no tutorial and no
## advisor in this game, so anything the reference omits is something a player has
## no way to find out.
##
## Composing it in GDScript makes an *omission* unrepresentable: the screen walks
## `building_kind_count()` rather than a list of entries somebody wrote. What is
## still representable is a section that composes to nothing at all -- a bad
## index, an empty roster, a packed read whose stride moved -- and that failure
## looks exactly like a screen nobody has opened yet. `cargo test` cannot see
## GDScript, so it is checked here.
func _check_reference() -> void:
	reference.open(republic)
	var names := ["how it works", "goods", "buildings", "vehicles", "ways and lines"]
	# Sampled from the rosters rather than typed: a name written here would be a
	# second copy of the table, which is the whole thing the reference exists not
	# to be. The *last* of each, because a truncated walk still emits the first.
	#
	# A list per section rather than one name, because the last section carries
	# two rosters: the grades and the kinds of line. One sample would have gone
	# on passing with every line in the game missing from the document.
	var wanted := [
		[],
		[String(republic.resource_names()[republic.resource_names().size() - 1])],
		[String(republic.building_kind_name(republic.building_kind_count() - 1))],
		[String(republic.vehicle_kind_names()[republic.vehicle_kind_names().size() - 1])],
		[
			String(republic.grade_names()[republic.grade_names().size() - 1]),
			String(republic.utility_names()[republic.utility_names().size() - 1]),
		],
	]
	var sampled := 0
	for i in names.size():
		var text: String = reference.section_text(i)
		if text.strip_edges() == "":
			printerr("reference check FAILED: the %s section is empty" % names[i])
			return
		for name in wanted[i]:
			sampled += 1
			if not text.contains(String(name)):
				printerr("reference check FAILED: %s is not in the %s section" % [
					name, names[i],
				])
				return
	print("reference check ok: %d sections, and the last of %d rosters is in one" % [
		names.size(), sampled,
	])


## Click a building, read it back, and pull it down.
##
## **The only reason this is worth a CI job is that none of it can be seen from
## Rust.** `cargo test` proves `building_at` answers the footprint rule and proves
## `Demolish` refuses a site with a crew on it. What only the shell can get wrong
## is the round trip: whether the point under a cursor reaches the pick, whether
## the packed state comes back the width the panel slices it at, and whether the
## demolish button reaches the command. A stride that moved would show a panel of
## plausible wrong numbers and no test in either language would see it.
func _check_inspector() -> void:
	# The build check contracted a house; the labour check contracted a post.
	# Both are standing, so there is something to click on.
	var count := republic.building_count()
	if count < 1:
		printerr("inspector check FAILED: nothing is standing to inspect")
		return
	var state: PackedFloat32Array = republic.building_state(count)
	var stride := int(inspector.STATE_STRIDE)
	if state.size() != stride:
		printerr("inspector check FAILED: the state read is %d wide, the panel slices %d" % [
			state.size(), stride,
		])
		return
	# The pick, through the same binding the click uses, aimed at the centre the
	# state read just reported. A building must be selectable where it says it is.
	var picked := republic.building_at(state[int(inspector.AT_X)], state[int(inspector.AT_Y)])
	if picked != count:
		printerr("inspector check FAILED: building %d stands at %.0f, %.0f and the pick found %d" % [
			count, state[int(inspector.AT_X)], state[int(inspector.AT_Y)], picked,
		])
		return
	# And open ground, which must find nothing: a pick that answered a building
	# everywhere would pass the line above and be useless.
	if republic.building_at(-500.0, -500.0) != 0:
		printerr("inspector check FAILED: the pick found a building off the map")
		return

	var words: String = inspector.check(republic)
	if words != "":
		printerr("inspector check FAILED: %s" % words)
		return

	# Demolition, which nothing could do at all before this milestone -- and the
	# settings screen shipped a `Confirm before demolishing` row that nothing
	# read, which is a control for a verb that did not exist.
	var why := String(republic.demolish(count))
	if why != "":
		printerr("inspector check FAILED to demolish: %s" % why)
		return
	if republic.building_count() != count - 1:
		printerr("inspector check FAILED: it was pulled down and the count did not move")
		return
	# And the refusal path, because a verb that accepts everything is a verb that
	# is not reading the rules. Nothing was ever building 9999.
	if String(republic.demolish(9999)) == "":
		printerr("inspector check FAILED: a building that does not exist was demolished")
		return
	print("inspector check ok: picked, read %d figures, and pulled it down" % state.size())


## Put a way and a line on the map that nobody has finished yet.
##
## Reached only from `--screen town`. Both go through the bindings the tools
## press, so what stands on the map is what a player's clicks would have left
## there — and a few days after them, so the ribbons are part-filled rather than
## sitting at zero, which is the state the drawing has to be right about.
func _order_some_works() -> void:
	var centre := Vector2(republic.centre_x(), republic.centre_y())
	# Out of the town rather than through it, so the sites are not drawn under
	# the buildings the rest of the capture is checking.
	#
	# **Long enough that four days does not finish them**, which is the whole
	# point: a site that completes between the order and the shutter is a
	# finished road, and the thing being looked at is the part-built ribbon —
	# solid up to where the crew have got, a marked-out strip beyond it. A short
	# dirt track is twenty builder-days and this town has ten builders.
	var _way := republic.order_way(
		0, centre.x + 200.0, centre.y + 200.0, centre.x + 2400.0, centre.y + 1900.0, false
	)
	var _line := republic.order_line(
		0, centre.x + 200.0, centre.y - 150.0, centre.x + 2000.0, centre.y - 1500.0
	)
	# One day, which on a way this long leaves it part-made — the state worth
	# looking at. Ten builders finish a 2.8 km dirt track inside a week, and a
	# capture taken after that is a capture of a road.
	republic.advance_days(1)


## The first vehicle with a job in hand, or 0 if the whole fleet is parked.
##
## Reached only from `--screen vehicle`. Reads the same packed state the panel
## slices, so it cannot pick a vehicle the panel would then fail to compose.
func _a_vehicle_on_the_move() -> int:
	var job := int(vehicle_panel.JOB)
	for i in republic.vehicle_count():
		var state: PackedFloat32Array = republic.vehicle_state(i)
		if state.size() > job and state[job] >= 0.0:
			return i
	return 0


## Click a lorry and read it back.
##
## The same argument the inspector check makes, about the other half of the map.
## `cargo test` proves the fleet dispatches and proves `bog_chance` is a
## probability; what only the shell can get wrong is whether the point under a
## cursor reaches a vehicle, and whether the packed state comes back the width
## the panel slices it at. A stride that moved would show a panel of plausible
## wrong numbers, in Rust and in GDScript both, and neither language's tests
## would see it.
func _check_vehicle() -> void:
	# A republic owns no vehicles at its founding -- they are bought by garages
	# out of an economy that has to exist first -- so the fixture town is what
	# puts something on the road to click on.
	republic.found_fixture_town(143)
	# Long enough for the garages to have taken delivery and for the freight
	# system to have sent something somewhere. A parked lorry would still exercise
	# the pick, but not the journey columns.
	republic.advance_days(3)
	if republic.vehicle_count() < 1:
		printerr("vehicle check FAILED: three days of a working town raised no vehicles")
		return
	# Open ground, which must find nothing: a pick that answered a vehicle
	# everywhere would pass the panel's own check and be useless.
	if republic.vehicle_at(-5000.0, -5000.0, 30.0) >= 0:
		printerr("vehicle check FAILED: the pick found a vehicle off the map")
		return
	var words: String = vehicle_panel.check(republic)
	if words.begins_with("vehicle check ok"):
		print(words)
	else:
		printerr(words)


## Write a trade rule, move it, and answer a tender.
func _check_trade() -> void:
	var before: int = republic.trade_rules().size()
	var why := String(republic.add_trade_rule(0, 0, false, 0.0))
	if why != "":
		printerr("trade check FAILED: %s" % why)
		return
	if republic.add_trade_rule(1, 1, true, 40.0) != "":
		printerr("trade check FAILED: a buy rule was refused")
		return
	var rules: PackedFloat32Array = republic.trade_rules()
	if rules.size() != before + 8:
		printerr("trade check FAILED: two rules added and the plan grew by %d floats" % [
			rules.size() - before,
		])
		return
	# The order **is** the decision, so the move is what actually needs checking:
	# a customs house short of throughput serves the first rule first.
	if String(republic.move_trade_rule(1, 0)) != "":
		printerr("trade check FAILED: a rule could not be moved")
		return
	rules = republic.trade_rules()
	if rules[int(trade_screen.RULE_ACTION)] < 0.5:
		printerr("trade check FAILED: the buy rule was moved to the front and is not there")
		return
	# And the refusal path.
	if String(republic.move_trade_rule(0, 99)) == "":
		printerr("trade check FAILED: a rule was moved to a position that does not exist")
		return
	# Both of them, one at a time: after the first withdrawal the second rule is
	# at position 0, which is also the only way to prove the list actually shrank.
	for _each in 2:
		if String(republic.remove_trade_rule(0)) != "":
			printerr("trade check FAILED: a rule could not be withdrawn")
			return
	if republic.trade_rules().size() != before:
		printerr("trade check FAILED: two rules added and two withdrawn left %d floats" % [
			republic.trade_rules().size(),
		])
		return
	var words: String = trade_screen.check(republic)
	if words != "":
		printerr("trade check FAILED: %s" % words)
		return
	# Tenders arrive every other month and a freshly founded republic has none,
	# so what is reachable here is the refusal — which is the half only the shell
	# can get wrong.
	if String(republic.accept_contract(1)) == "":
		printerr("trade check FAILED: a tender that was never offered was accepted")
		return
	print("trade check ok: the export plan is the republic's to write and to rank")


## Borrow, read the debt back, and repay it.
func _check_finance() -> void:
	var tiers: PackedFloat32Array = republic.loan_tiers(0)
	if tiers.size() < int(finance_screen.T_STRIDE):
		printerr("finance check FAILED: the blocs offer no advances at all")
		return
	var before: PackedFloat32Array = republic.purse()
	var why := String(republic.take_loan(0, 0))
	if why != "":
		printerr("finance check FAILED: %s" % why)
		return
	var after: PackedFloat32Array = republic.purse()
	if after[0] <= before[0]:
		printerr("finance check FAILED: an advance was taken and the treasury did not move")
		return
	if republic.owed_to(0) <= 0.0:
		printerr("finance check FAILED: money was advanced and nothing is owed")
		return
	var loans: PackedFloat32Array = republic.loans()
	var loan_stride := int(finance_screen.L_STRIDE)
	if loans.size() != loan_stride:
		printerr("finance check FAILED: one advance reads as %d floats, the screen slices %d" % [
			loans.size(), loan_stride,
		])
		return
	# One advance per bloc, which is what stops the mechanic being a money
	# printer with a deadline nobody has to meet.
	if String(republic.take_loan(0, 0)) == "":
		printerr("finance check FAILED: one bloc advanced twice")
		return
	if String(republic.can_take_loan(0, 0)) == "":
		printerr("finance check FAILED: the pre-flight says yes where the command says no")
		return
	if String(republic.repay_loan(0, republic.owed_to(0))) != "":
		printerr("finance check FAILED: an advance could not be repaid")
		return
	if republic.owed_to(0) > 0.0 or republic.loans_cleared() < 1:
		printerr("finance check FAILED: it was repaid in full and is still on the books")
		return
	print("finance check ok: borrowed %.0f, owed it, and cleared it" % tiers[0])


## Read back the record of everything the checks above have just done.
##
## The journal is the one screen whose subject is made by the other checks: by the
## time this runs, this republic has contracted two buildings, laid a road, set
## the working day, ranked a workplace, written trade rules, borrowed and repaid.
## So this asserts against a journal with something in it rather than an empty
## one, which is what makes the phrase table worth checking at all.
func _check_journal() -> void:
	var words: String = journal_screen.check(republic)
	if words != "":
		printerr("journal check FAILED: %s" % words)
		return
	var total: int = republic.journal_len()
	if total < 5:
		printerr("journal check FAILED: %d orders recorded after a run that issued many" % total)
		return
	var page: PackedFloat32Array = republic.journal_page(0, total)
	var journal_stride := int(journal_screen.J_STRIDE)
	@warning_ignore("integer_division")
	var entries: int = page.size() / journal_stride
	if entries != total:
		printerr("journal check FAILED: %d orders and a page of %d" % [total, entries])
		return
	# Every verb the run produced must be one the screen has a phrase for. A verb
	# past the end of the table prints a blank line, which reads as an order that
	# did nothing rather than as a bug.
	for i in entries:
		var verb := int(page[i * journal_stride + int(journal_screen.J_VERB)])
		if verb < 0 or verb >= journal_screen.PHRASES.size():
			printerr("journal check FAILED: order %d has verb %d and there are %d phrases" % [
				i, verb, journal_screen.PHRASES.size(),
			])
			return
	print("journal check ok: %d orders, every one of them a sentence" % total)


## Contract a building and check one goes up.
##
## **This is the only way to play now**, so it is worth a line in the check that
## CI runs. A blank map has no crews and no materials, so if contracting is
## broken there is no path from a founded republic to a built anything — and the
## symptom would be an empty map that stays empty, which looks exactly like a
## republic nobody has got round to building yet.
##
## Goes through the same `contract` binding the build menu calls, rather than
## through the simulation directly, because the binding is the part only the
## shell can get wrong.
func _check_building() -> void:
	var centre := republic.map_extent() * 0.5
	var before := republic.building_count()
	# Walk outward for ground that will take it: the site the founding picked is
	# chosen for coal, not for being flat.
	var why := "nowhere tried"
	for ring in 12:
		var at: float = centre + float(ring) * 90.0
		why = String(republic.contract(0, at, at, 0))
		if why == "":
			break
	if why != "":
		printerr("build check FAILED: %s" % why)
		return
	if republic.building_count() != before + 1:
		printerr("build check FAILED: the contract was accepted and no site appeared")
		return
	print("build check ok: contracted %s" % republic.building_kind_name(0))


## Set the working day and read it back off a building.
##
## The roster is three levels deep and every level is resolved on the Rust side,
## so the thing only the shell can get wrong is whether a number the player types
## reaches the simulation and comes back changed. A refusal is a sentence, so a
## silently-ignored setter looks identical to a working one from GDScript — which
## is exactly the failure this line catches.
func _check_labour() -> void:
	var was: float = republic.national_shift_hours()
	var why := String(republic.set_national_shift_hours(12.0))
	if why != "":
		printerr("labour check FAILED: %s" % why)
		return
	if not is_equal_approx(republic.national_shift_hours(), 12.0):
		printerr("labour check FAILED: the standard was accepted and did not change")
		return
	# And the refusal path, because a setter that accepts everything is a setter
	# that is not reading the rules.
	if String(republic.set_national_shift_hours(40.0)) == "":
		printerr("labour check FAILED: a forty-hour shift was accepted")
		return
	var _restore := republic.set_national_shift_hours(was)

	# And the standing, which is the lever that decides who is manned at all when
	# the republic is short of hands.
	#
	# **Not read back through `workplaces`, and the first version of this check
	# was silently useless because it tried.** That view lists only *built*
	# workplaces, and a `--check` republic has founded an empty map moments
	# earlier, so the array was always empty, the round-trip never ran, and the
	# line printed "ok" every time. A check that cannot reach its subject and
	# says nothing is worse than no check. What is actually reachable here is
	# both ends of the binding — a standing that is accepted and one that is
	# refused — and those are what only the shell can get wrong.
	if republic.priority_names().size() < 2:
		printerr("labour check FAILED: the republic names no standings")
		return
	# A workplace, so the accept path has a subject. Index 3 is the woodcutter
	# post; the house the build check contracts has no roster at all.
	var centre := republic.map_extent() * 0.5
	var sited := "nowhere tried"
	for ring in 12:
		var at: float = centre - float(ring) * 90.0
		sited = String(republic.contract(3, at, at, 0))
		if sited == "":
			break
	if sited != "":
		printerr("labour check FAILED: nowhere would take a workplace: %s" % sited)
		return
	# Ids are one-based and handed out in commissioning order, so the post just
	# contracted is the last one. `building_count() - 1` is the house, and using
	# it is what made the first run of this check fail against a working setter.
	var post := republic.building_count()
	var moved := String(republic.set_priority(post, 0))
	if moved != "":
		printerr("labour check FAILED: a standing on a workplace was refused: %s" % moved)
		return
	# And the refusal path, because a setter that accepts everything is a setter
	# that is not reading the rules. Building 1 is the house the build check
	# contracted, and a house is nobody's workplace.
	if String(republic.set_priority(1, 0)) == "":
		printerr("labour check FAILED: a standing was accepted on a building with no roster")
		return
	print("labour check ok: the working day and the labour plan are the republic's to set")


## Lay a road, and check the lamp rule refuses what it should.
##
## A road was orderable from the simulation and from nowhere else for six
## milestones — the binding existed and no `.gd` file called it. On a map that
## starts empty that is the difference between a republic and a set of buildings
## nothing can reach, so it gets a line in the check that CI runs.
func _check_road() -> void:
	var centre := republic.map_extent() * 0.5
	var before := republic.road_length_km()
	# Dirt, because a founded republic has no gravel and no asphalt, and this is
	# a check that the order goes through rather than that the crew finish it.
	var why := "nowhere tried"
	for ring in 8:
		var a: float = centre + float(ring) * 120.0
		why = String(republic.order_way(0, a, a, a + 600.0, a, false))
		if why == "":
			break
	if why != "":
		printerr("road check FAILED: %s" % why)
		return
	if republic.road_site_count() < 1:
		printerr("road check FAILED: the order was accepted and no site appeared")
		return
	# And the refusal that makes lamps a paved-only thing. A dirt track with
	# street lighting has to come back with a sentence, not a road.
	if String(republic.order_way(0, centre, centre, centre + 600.0, centre, true)) == "":
		printerr("road check FAILED: a dirt track was given street lamps")
		return
	# The *site* count, not the network length: nothing routes over a road until
	# the crew have finished it, so a freshly ordered road adds zero kilometres
	# of drivable way and saying otherwise would be reporting a road that is not
	# there yet.
	print("road check ok: %d way(s) ordered from %.1f km of road, and lamps stay on the tarmac" % [
		republic.road_site_count(), before
	])


## String a line, and check every kind of them has a colour and a price.
##
## `order_line` was in the state `order_way` had been in for six milestones, and
## this is the line in CI that stops it going back: a republic that cannot string
## a wire is a republic whose power stations light nothing but themselves.
##
## The roster checks are the other half. `Utility::ALL` has grown twice, and both
## times something on this side kept its old length -- a colour list of four
## against five kinds draws the fifth in fallback white, which reads as a
## rendering choice rather than as a missing row.
func _check_lines() -> void:
	var kinds: PackedStringArray = republic.utility_names()
	if kinds.size() < 1:
		printerr("line check FAILED: the republic strings nothing at all")
		return
	if UTILITY_TONES.size() != kinds.size():
		printerr("line check FAILED: %d kinds of line and %d colours for them" % [
			kinds.size(), UTILITY_TONES.size(),
		])
		return
	var table: PackedFloat32Array = republic.utility_table()
	var stride: int = republic.utility_stride()
	if table.size() != kinds.size() * stride:
		printerr("line check FAILED: %d kinds of line and a table of %d floats at %d wide" % [
			kinds.size(), table.size(), stride,
		])
		return

	var centre := republic.map_extent() * 0.5
	# Power, because it is the one every republic strings first and the one whose
	# absence is felt on day one. A check that the order goes through rather than
	# that the crew finish it -- nothing carries current until the last
	# builder-day is paid.
	var why := "nowhere tried"
	for ring in 8:
		var a: float = centre + float(ring) * 140.0
		why = String(republic.order_line(0, a, a, a + 500.0, a))
		if why == "":
			break
	if why != "":
		printerr("line check FAILED: %s" % why)
		return
	if republic.utility_sites().size() < 1:
		printerr("line check FAILED: the order was accepted and no site appeared")
		return
	print("line check ok: %d kind(s) of line, each priced, and a power line ordered" % kinds.size())


## Write settings, read them back through a fresh store, and check they survived.
##
## "Settings that work" is a clause of the release standard that nothing checked.
## Rendering the screen shows that the controls appear and says nothing at all
## about whether a value written on one run is there on the next, which is the
## half a player would notice.
##
## Round-tripped through a scratch file rather than `settings.cfg`, because
## proving a write works by overwriting somebody's real configuration is a bad
## trade — and this runs on the developer's machine as well as on CI.
func _check_settings() -> void:
	var scratch := "user://settings-check.cfg"
	var written := Store.new()
	# Values no default holds, so a store that quietly dropped the write and
	# handed back its own default would fail here rather than pass.
	written.set_value("audio/master", 0.37)
	var flipped := not bool(Store.new().get_value("display/vsync"))
	written.set_value("display/vsync", flipped)
	written.save_to_disk(scratch)

	var read_back := Store.new()
	read_back.load_from_disk(scratch)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(scratch))

	if not is_equal_approx(float(read_back.get_value("audio/master")), 0.37):
		printerr("settings check FAILED: audio/master came back %s" % [
			read_back.get_value("audio/master")])
		return
	if bool(read_back.get_value("display/vsync")) != flipped:
		printerr("settings check FAILED: display/vsync came back at its default")
		return
	print("settings check ok")


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

	var file_name: String = SaveFiles.name_for(
		before["name"], before["date"], republic.save_version()
	)
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
		"0-5 speed  ·  space pause  ·  click a building or a lorry  ·  "
		+ "B build  L labour  C trade  M finance  J journal  Y weather  ·  esc menu  ·  "
		+ "F none  G going  T tracks  R survey  P smoke  N snow  ·  "
		+ "WASD pan  ·  right-drag orbit  ·  wheel zoom"
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
	_sites_shown = -1
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
	build_menu.chose.connect(_begin_placement)
	build_menu.chose_way.connect(_begin_way)
	build_menu.chose_line.connect(_begin_line)
	build_menu.closed.connect(func(): _set_screen(Screen.PLAYING))
	labour_screen.closed.connect(func(): _set_screen(Screen.PLAYING))
	trade_screen.closed.connect(func(): _set_screen(Screen.PLAYING))
	finance_screen.closed.connect(func(): _set_screen(Screen.PLAYING))
	journal_screen.closed.connect(func(): _set_screen(Screen.PLAYING))
	year_screen.closed.connect(func(): _set_screen(Screen.PLAYING))
	# The inspector closing is not a screen change: the republic was never
	# stopped for it, because it is a panel over a running map.
	inspector.closed.connect(func(): hud.set_placing("", ""))


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
	trade_screen.visible = screen == Screen.TRADE
	finance_screen.visible = screen == Screen.FINANCE
	journal_screen.visible = screen == Screen.JOURNAL
	year_screen.visible = screen == Screen.YEAR
	hud.visible = screen == Screen.PLAYING
	# The inspector rides with the HUD rather than with the screen it is over: it
	# is a panel about a building on the map, and a full sheet drawn over the map
	# has covered the building.
	inspector.visible = screen == Screen.PLAYING and inspector.is_open()
	vehicle_panel.visible = screen == Screen.PLAYING and vehicle_panel.is_open()

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
		Screen.TRADE:
			trade_screen.open(republic)
		Screen.FINANCE:
			finance_screen.open(republic)
		Screen.JOURNAL:
			journal_screen.open(republic)
		Screen.YEAR:
			year_screen.open(republic)


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
func _open_named_screen(spec: String) -> void:
	# **A screen with tabs is more than one screen to look at.** `--screen
	# reference:2` opens the third of them, for the same reason `--pitch`, `--hour`
	# and `--at` exist: a capture you cannot aim is a check that answers without
	# looking, and four fifths of the reference could not be rendered at all.
	var parts := spec.split(":")
	var name := parts[0]
	var page := int(parts[1]) if parts.size() > 1 else 0
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
			reference.show_page(page)
		"radio":
			_open_stacked(Screen.RADIO, Screen.MENU)
		"saves":
			_open_saves(SaveFiles, Screen.MENU)
		"pause":
			# A pause overlay over a real republic, which is the only way it is
			# ever seen.
			_found_default()
			_set_screen(Screen.PAUSED)
		"build":
			# Over a real republic too, because the menu reads the treasury and a
			# purse line with no world behind it is the one part of this screen a
			# capture would not be checking.
			_found_default()
			build_menu.open(republic)
		"town":
			# Not a screen: the map, with the fixture town standing on it. The
			# only way a capture ever sees buildings, a lit street or lamp
			# geometry, because a founded republic is an empty field.
			_found_default()
			republic.found_fixture_town(143)
			republic.advance_days(2)
			# **Works in progress, because a finished republic never shows
			# them.** A half-built way and an unstrung line are their own
			# geometry — a ribbon that fills in from the near end, a thin span
			# lifted clear — and Godot culls back faces, so either of them wound
			# the wrong way renders as nothing at all and reads as "not built
			# yet" rather than as a fault. There is no way to see that except in
			# a frame, and no other capture puts one on the map.
			_order_some_works()
			if _start_hour >= 0.0:
				republic.advance_to_hour(_start_hour)
			_enter_republic()
			# **After `_enter_republic`, not before.** Entering frames the camera
			# on the republic, so the first version of `--at` was applied and
			# then silently undone — a capture flag that reaches nothing, which
			# is the whole thing `--at` exists to stop happening.
			_apply_view_flags()
			_set_screen(Screen.PLAYING)
		"inspector":
			# A panel about one building needs a building. The fixture town is
			# the only thing that stands any up without a player, and a capture
			# of an empty inspector would be a capture of a heading — every
			# layout bug this project has shipped was in a row.
			_found_default()
			republic.found_fixture_town(143)
			republic.advance_days(2)
			_enter_republic()
			_apply_view_flags()
			_set_screen(Screen.PLAYING)
			# `page` picks which building, so a capture can be aimed at a house,
			# a works or a Construction Office — they compose different columns,
			# and a capture that only ever saw one of them would be checking a
			# third of the panel.
			inspector.open(republic, _store, maxi(page, 1))
		"vehicle":
			# A panel about a lorry needs a lorry, and a lorry needs an economy
			# that bought it. The fixture town plus a few days is the only thing
			# that puts one on a road without a player, and a capture of an empty
			# vehicle panel would be a capture of a heading.
			#
			# **Three days is not enough and that is the whole difficulty.** The
			# fixture town is dealt with full bins, so nothing needs fetching
			# until something has been consumed -- at three days every lorry in
			# the republic is parked and the journey columns are a row of
			# dashes. A month of the town eating its stores is what puts freight
			# on the road.
			_found_default()
			republic.found_fixture_town(143)
			republic.advance_days(30)
			_enter_republic()
			_apply_view_flags()
			_set_screen(Screen.PLAYING)
			# `page` picks which vehicle, and with no page given the harness
			# aims itself at one that is actually out. Nearly every lorry in a
			# republic is parked at any moment, so `vehicle:0` is reliably a
			# panel of dashes -- and half a panel is half a check.
			vehicle_panel.open(republic, page if page > 0 else _a_vehicle_on_the_move())
		"trade", "finance":
			# **Both of these render an empty table on a freshly founded
			# republic**, and a capture of a screen with no rows checks nothing —
			# every layout bug this project has shipped was in a row. So the same
			# handful of real orders the journal capture gives is given here: it
			# writes two trade rules and takes an advance, which is exactly the
			# state these two screens exist to show.
			_found_default()
			_play_a_few_orders()
			_set_screen(Screen.TRADE if name == "trade" else Screen.FINANCE)
		"year":
			# **A year screen needs a year, and a fresh founding has nine days.**
			# On day one the chart is a single point, the frost figures say
			# "none" and there is no previous year at all -- a capture of that
			# checks the title block and nothing else. Advancing through a whole
			# winter is what puts a curve on the screen and fills both columns,
			# which is the layout that has to be looked at.
			#
			# 800 days rather than a round 400: the founding is the first of
			# March, so a whole calendar year is only behind the republic once
			# the second January has come round -- at 400 the right-hand column
			# still reads "nothing on record yet", which is the half of this
			# screen most worth looking at. 800 lands in the following May, so
			# both columns have a winter in them and the two curves overlap
			# rather than one of them being a single point.
			_found_default()
			republic.advance_days(800)
			_set_screen(Screen.YEAR)
		"journal":
			# **The fixture town is no use here and that is worth stating.** It
			# stands buildings up directly, without issuing a command for any of
			# them, so a journal over it has one entry — the republic's name. A
			# capture of a screen with no rows checks nothing.
			#
			# So this issues real orders through the same bindings a player
			# presses, which is the only honest way to make a record of orders:
			# they *are* orders, given by a script instead of a hand.
			_found_default()
			_play_a_few_orders()
			_set_screen(Screen.JOURNAL)
		"labour":
			# Needs a republic with workplaces standing in it: on a blank map the
			# table under the two policy controls is empty, and a capture of a
			# screen with no rows checks nothing — every layout bug this project
			# has shipped was in a row. `found_fixture_town` is the nineteen-
			# building hand the founding used to deal, kept as a fixture, and it
			# is reachable from here and from nowhere a player goes.
			_found_default()
			republic.found_fixture_town(143)
			# A day, so the labour pass has run and the manning column reports
			# people rather than a republic of empty posts.
			republic.advance_days(2)
			_enter_republic()
			labour_screen.open(republic)
		_:
			push_error("no screen called '%s'" % name)
			_set_screen(Screen.MENU)


## Give a handful of real orders, so a capture of the journal has rows in it.
##
## Reached only from `--screen journal`. Every line here goes through a binding a
## player presses, so what lands in the record is what a player's would be —
## there is no way to write an entry the journal would not otherwise hold, and
## there should not be.
func _play_a_few_orders() -> void:
	var centre := republic.map_extent() * 0.5
	for ring in 8:
		var at: float = centre + float(ring) * 90.0
		if String(republic.contract(0, at, at, 0)) == "":
			break
	for ring in 8:
		var at: float = centre - float(ring) * 90.0
		if String(republic.contract(3, at, at, 0)) == "":
			break
	var _road := republic.order_way(0, centre, centre, centre + 700.0, centre, false)
	var _hours := republic.set_national_shift_hours(10.0)
	var _sell := republic.add_trade_rule(0, 0, false, 0.0)
	var _buy := republic.add_trade_rule(1, 1, true, 40.0)
	var _rank := republic.move_trade_rule(1, 0)
	var _advance := republic.take_loan(0, 1)


func _apply_interface_scale() -> void:
	# Applied to the whole viewport rather than per control, so one number moves
	# every screen and nothing can be missed.
	var scale: float = _store.interface_scale()
	if not is_equal_approx(scale, 1.0):
		get_window().content_scale_factor = scale


func _process(delta: float) -> void:
	if _screen == Screen.PLAYING:
		_run_the_sun()
		_update_ghost()
		_update_span()
		_refresh_buildings()
		_refresh_roads()
		_refresh_road_sites()
		_refresh_lines()
		_refresh_ways()
		_refresh_vehicles()
		_refresh_newcomers()
		_refresh_overlay()
		_refresh_status()
		_refresh_inspector()
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
	var file_name: String = SaveFiles.autosave_name(
		String(republic.republic_name()), republic.save_version()
	)
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
## **This turns vsync off itself, and it has to.** With vsync on, every p50 comes
## back as exactly 16.67 ms whatever the load, so a scene that is drowning
## reports a healthy number -- it silently invalidated a whole probe run once.
## The old defence was `window/vsync/vsync_mode=0` in project.godot, and a
## comment here saying so was load-bearing. It stopped being true the moment M12
## gave the game a settings screen: `settings_store.gd` defaults `display/vsync`
## to true and `main.gd` applies it at startup, a few lines before any of this
## runs. Measured on 2026-07-30 -- the first bench of the art work came back
## 16.67/16.67, which is the tell.
##
## So the benchmark takes responsibility for its own preconditions rather than
## trusting a setting three files away that somebody else owns. A measurement
## tool that can be silently disarmed by an unrelated feature is worse than none.
##
## Real-time is the thesis, so "does it hold frame rate while the republic is
## actually being simulated" is not a nice-to-have measurement. It is the
## constraint the renderer exists under.
func _maybe_bench(delta: float) -> void:
	if _bench_frames <= 0:
		return
	if _bench_times.is_empty():
		DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
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
	# Placing is a mouse mode, so it gets first refusal on clicks. Left commits,
	# right cancels — the convention every builder uses, and the reason right
	# drag still orbits is that the camera only sees motion events.
	if _build_kind >= 0 and event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_LEFT:
			_commit_placement()
			get_viewport().set_input_as_handled()
			return
		if event.button_index == MOUSE_BUTTON_RIGHT:
			_cancel_placement()
			get_viewport().set_input_as_handled()
			return

	if _laying() and event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_LEFT:
			_click_span()
			get_viewport().set_input_as_handled()
			return
		if event.button_index == MOUSE_BUTTON_RIGHT:
			_cancel_span()
			get_viewport().set_input_as_handled()
			return

	# **The pick, and it is last of the three mouse modes on purpose.** Placing a
	# building and laying a road both take a click that means something else, so
	# selecting only happens when there is nothing in hand — which is also what
	# makes right-click unambiguous: it cancels what you are holding, or it
	# closes the panel.
	if _screen == Screen.PLAYING and event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_LEFT:
			_pick_building()
			get_viewport().set_input_as_handled()
			return
		if event.button_index == MOUSE_BUTTON_RIGHT and inspector.is_open():
			inspector.close_panel()
			get_viewport().set_input_as_handled()
			return

	if not (event is InputEventKey and event.pressed and not event.echo):
		return

	# Escape is the one key that means something on every screen, and what it
	# means depends on where you are: out of the game into the pause overlay, and
	# out of a stacked screen back to whatever opened it.
	if event.keycode == KEY_ESCAPE:
		# Placing takes priority: escape out of the ghost first, and only out of
		# the game once there is nothing in hand.
		if _build_kind >= 0:
			_cancel_placement()
			return
		if _laying():
			_cancel_span()
			return
		# Then the inspector, for the same reason: escape means "put down what
		# you are holding" before it means "leave the game".
		if _screen == Screen.PLAYING and inspector.is_open():
			inspector.close_panel()
			return
		if _screen == Screen.PLAYING and vehicle_panel.is_open():
			vehicle_panel.close_panel()
			return
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
	if event.keycode == KEY_B:
		build_menu.open(republic)
	elif event.keycode == KEY_L:
		labour_screen.open(republic)
	elif event.keycode == KEY_C:
		# C for customs rather than T, which the tracks overlay has held since
		# before there was a trade screen to give it to.
		_set_screen(Screen.TRADE)
	elif event.keycode == KEY_M:
		_set_screen(Screen.FINANCE)
	elif event.keycode == KEY_J:
		_set_screen(Screen.JOURNAL)
	elif event.keycode == KEY_Y:
		_set_screen(Screen.YEAR)
	elif speeds.has(event.keycode):
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
	# Not during a capture or a benchmark. Both are launched from a terminal that
	# keeps focus, so the game reliably never has it, and the pause overlay would
	# slide over the frame being captured -- which is exactly what happened on the
	# first render of the art work: a shot meant to show the world came back
	# showing the pause menu, and only because the run before it happened to win
	# the focus race did the two look different at all.
	#
	# A measurement tool that photographs its own UI is not measuring anything,
	# and the failure is intermittent, which is worse.
	if _shot_path != "" or _bench_frames > 0:
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
			"--pitch":
				if i + 1 < args.size():
					_view_pitch = float(args[i + 1])
			"--hour":
				if i + 1 < args.size():
					_start_hour = float(args[i + 1])
			"--at":
				# Where to point the camera, in map metres: `--at 900,2400`.
				#
				# **A capture you cannot aim is a check that answers without
				# looking.** The water work landed with the camera framed on the
				# republic and the river running along the far edge of the map,
				# so the only frames available of a brand-new water shader had it
				# forty pixels wide and edge-on. Same lesson as `--hour` and as
				# `--pitch` before it, for the third time.
				if i + 1 < args.size():
					var parts := args[i + 1].split(",")
					if parts.size() == 2:
						_view_at = Vector2(float(parts[0]), float(parts[1]))
			"--fixture":
				# Stand the nineteen-building fixture town up before capturing.
				#
				# **A capture of an empty map cannot show a building, a lorry or a
				# road**, and those are most of what there is to look at. `--shot`
				# founds a blank posting and the shell has no player in it, so
				# `--advance` ticks a republic that never builds anything: every
				# world capture taken so far has been six kilometres of grass.
				# The labour screen already reaches for this fixture for the same
				# reason, and it is the only way either of them gets a subject.
				_fixture_town = true
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


## Keep the shadow range around the camera.
##
## **This is the fix for the reason this game had no shadows at all.**
## `shadow_enabled` has been true since M3, and `directional_shadow_max_distance`
## was never set -- so it sat at Godot's default of **100 metres** while the
## camera opens at about 1,080 m on a 6 km map and can boom out to 9,600 m.
## Every shadow was being culled before it was drawn. Nothing errored, the light
## reported itself as casting, and three art directions were chosen from renders
## that could not contain a shadow.
##
## The range has to follow the boom because it is measured from the camera and a
## fixed value cannot serve both ends of a 240x zoom: a range wide enough for the
## whole map spreads four cascades so thin that a lorry casts a smear, and one
## tight enough for a lorry leaves the map bare. `distance_changed` is what makes
## this cheap -- it fires on camera moves rather than every frame.
func _fit_shadows(metres: float) -> void:
	var sun: DirectionalLight3D = $Sun
	# Far enough past the pivot to cover the ground behind it, capped so the
	# cascades keep usable resolution when the whole posting is in frame.
	sun.directional_shadow_max_distance = clampf(metres * 2.5, 400.0, 8000.0)
	# Bias scales with the range: what stops acne at 400 m detaches a shadow from
	# its building at 8 km, and the artefact you get for guessing is the one that
	# looks like the mesh is floating.
	sun.shadow_normal_bias = clampf(metres / 900.0, 1.0, 6.0)


## Sun, sky and air. Presentation only -- the weather the simulation models is a
## different thing entirely, and this does not read it.
## How high the sun sits at noon, and how far it swings across the day.
##
## Not an ephemeris. The republic has a latitude in the fiction and none in the
## simulation, and a real solar position would buy a shadow angle nobody can
## check against anything. What the arc has to do is be legible: the light moves
## while you watch it, the shadows swing, and dusk arrives.
const SUN_PEAK_DEGREES := 58.0

## The speed above which the sun stands still.
##
## **Noah's call, and the reason is physiological rather than aesthetic.** At
## speed 1 a day takes twenty-four real minutes and the movement reads as
## movement. At speed 5 a second buys eight hours, so the sun would cross the
## sky every three seconds — unpleasant to look at and a genuine
## photosensitivity risk. Above this the light is pinned at the hour it was
## frozen at, which is also the only honest thing to do: nothing in the
## simulation cares what o'clock it is.
const SUN_FOLLOWS_UP_TO := 2

## How dark it gets, `0.0` broad daylight to `1.0` the middle of the night.
##
## Written to the `night` global shader parameter, so every window in the
## republic reads it without the scene walking a hundred and thirty materials.
var _night := 0.0
## The live environment, so the day/night pass can reach into it.
var _environment: Environment = null
## Whether the sun has been placed at least once since the look was applied.
## Without it the early-out below would leave the light wherever `_apply_look`
## put it on the first frame of a run that starts at midnight.
var _sun_set_once := false
## The hour a capture run should stand at, or negative for whatever the founding
## gave it. See `Republic::advance_to_hour` for why this exists at all.
var _start_hour := -1.0
## Where a capture run should point the camera, or negative for the republic.
var _view_at := Vector2(-1.0, -1.0)


## Swing the sun, and take the light down with it.
##
## Real-time is this game's thesis and a sun that never moved was the largest
## thing on the screen quietly saying otherwise.
func _run_the_sun() -> void:
	if republic.speed() > SUN_FOLLOWS_UP_TO:
		return
	var f: float = republic.time_of_day()
	# Midnight at 0, sunrise at a quarter, noon at a half. `sin` of the whole
	# turn gives exactly that with no branch and no table.
	var height := sin(TAU * (f - 0.25))
	var day := clampf(height, 0.0, 1.0)
	var night := clampf(-height * 1.6, 0.0, 1.0)
	if absf(night - _night) < 0.002 and _sun_set_once:
		return
	_sun_set_once = true
	_night = night
	RenderingServer.global_shader_parameter_set("night", night)

	var sun: DirectionalLight3D = $Sun
	var env: Environment = _environment
	if env == null:
		return

	if height > 0.0:
		# Daylight. The sun rises in the east and sets in the west, and it goes
		# warm and low at both ends of that — which is where the long shadows
		# and the whole reason to watch a day pass come from.
		var elevation := SUN_PEAK_DEGREES * height
		sun.rotation_degrees = Vector3(-elevation, 90.0 + 360.0 * (f - 0.25), 0.0)
		var low := 1.0 - clampf(height * 2.2, 0.0, 1.0)
		sun.light_color = _look.sun_colour.lerp(Color(1.0, 0.62, 0.36), low)
		sun.light_energy = _look.sun_energy * clampf(0.15 + day * 1.05, 0.0, 1.6)
	else:
		# **A moon, not a sun below the horizon.** A DirectionalLight aimed up
		# from underneath lights the underside of everything and nothing else,
		# so the frame goes black with the sky still glowing. Mirroring the
		# elevation and going cold and faint is what every game does and what
		# a moon actually is.
		sun.rotation_degrees = Vector3(-SUN_PEAK_DEGREES * -height * 0.7, 270.0 + 360.0 * (f - 0.25), 0.0)
		sun.light_color = Color(0.62, 0.70, 0.92)
		sun.light_energy = _look.sun_energy * 0.09

	# Ambient follows, or a night with a bright sky-lit ground is just a day
	# with a blue filter on it.
	env.ambient_light_energy = _look.ambient_energy * lerpf(1.0, 0.14, night)
	# And a little more exposure at night, because a player still has to be able
	# to see the republic they are running. This is the one number here that is
	# a legibility decision rather than a physical one, and it is small.
	env.tonemap_exposure = _look.tonemap_exposure * lerpf(1.0, 1.18, night)


func _apply_look() -> void:
	var sun: DirectionalLight3D = $Sun
	sun.light_color = _look.sun_colour
	sun.light_energy = _look.sun_energy
	sun.rotation_degrees = Vector3(-_look.sun_elevation, _look.sun_azimuth, 0.0)
	sun.shadow_enabled = true
	# Four cascades blended, so the seam between them is not a visible line
	# across the ground on a map this size.
	sun.directional_shadow_mode = DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS
	sun.directional_shadow_blend_splits = true
	sun.directional_shadow_split_1 = 0.06
	sun.directional_shadow_split_2 = 0.16
	sun.directional_shadow_split_3 = 0.42
	sun.directional_shadow_fade_start = 0.9
	# A sun is half a degree across, and softening the penumbra with distance is
	# most of what makes a shadow look like light rather than a stencil.
	sun.light_angular_distance = 0.6
	if not rig.distance_changed.is_connected(_fit_shadows):
		rig.distance_changed.connect(_fit_shadows)
	_fit_shadows(rig.get_distance())

	# A custom sky shader rather than ProceduralSkyMaterial, which is a two-colour
	# gradient with a blob for the sun and was most of why the horizon read as two
	# flat bands meeting. `sky.gdshader` does Rayleigh and Mie scattering off the
	# sun's actual direction and puts cloud cover on top, so the sky changes with
	# the light instead of being painted behind it.
	var sky_mat := ShaderMaterial.new()
	sky_mat.shader = load("res://sky.gdshader")
	sky_mat.set_shader_parameter("zenith_colour", _look.sky_top)
	sky_mat.set_shader_parameter("horizon_colour", _look.sky_horizon)
	sky_mat.set_shader_parameter("ground_colour", _look.ground_horizon)
	sky_mat.set_shader_parameter("cloud_cover", _look.cloud_cover)

	var sky := Sky.new()
	sky.sky_material = sky_mat
	# The sky feeds ambient light through a cubemap, and regenerating it every
	# frame for clouds that drift at four thousandths of a unit a second is waste.
	# Incremental spreads the update over several frames.
	sky.process_mode = Sky.PROCESS_MODE_INCREMENTAL
	sky.radiance_size = Sky.RADIANCE_SIZE_128

	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	# Ambient off the sky rather than a flat colour, which is both the physical
	# answer and the one that keeps shading coherent when the sky changes: a
	# north-facing wall picks up the blue overhead and a south-facing one does
	# not. The old flat 0.75 was doing something else entirely -- filling every
	# shadow evenly, which is exactly how you erase form.
	env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	env.ambient_light_sky_contribution = 1.0
	env.ambient_light_energy = _look.ambient_energy
	env.tonemap_mode = Environment.TONE_MAPPER_ACES
	env.tonemap_exposure = _look.tonemap_exposure
	env.tonemap_white = 6.0
	# Distance fog rather than volumetric: the job is to give a 6 km map depth,
	# not to render weather, and it costs nothing.
	env.fog_enabled = true
	env.fog_mode = Environment.FOG_MODE_EXPONENTIAL
	env.fog_light_color = _look.fog_colour
	env.fog_density = _look.fog_density
	# Some sky affect, not none. Zero leaves a hard line where the ground meets
	# the sky, which is what the horizon looked like: two flat bands meeting.
	# Aerial perspective is what makes the far side of a 6 km map read as far
	# away rather than as the same ground painted smaller.
	env.fog_sky_affect = 0.18
	# 0.7 was too much once the ground had real texture on it: aerial perspective
	# blends the distance towards the sky, and against a bright sky it washed the
	# far half of the map to near-white. The flat-colour ground it was tuned
	# against had nothing to lose.
	env.fog_aerial_perspective = 0.45
	# No height fog. A first pass set `fog_height_density` to 0.6, which reads as
	# a modest number and is not one -- Godot's default is 0.0, and 0.6 made the
	# air below the reference height opaque. The frame came back as a flat pale
	# wash with the HUD floating on it and no world at all. Distance fog alone is
	# what this map wants.
	# Radius in metres. The default is 1 m, which from a kilometre up contributes
	# nothing at all -- it was enabled and invisible. At building scale it is
	# what settles a structure onto the ground.
	env.ssao_enabled = true
	env.ssao_radius = 12.0
	env.ssao_intensity = 2.0
	env.ssao_power = 1.5
	env.ssao_detail = 0.5
	# One bounce of indirect colour. Cheap next to SDFGI and it is what stops a
	# shadowed wall going flat grey.
	env.ssil_enabled = true
	env.ssil_radius = 24.0
	env.ssil_intensity = 1.0
	# Volumetric fog on top of the distance fog, for the air near the town to
	# have body and for the sun to rake through it. Godot caps the volume at
	# 1,024 m, so this is a local effect around the camera rather than the
	# map-wide depth cue -- that is what the exponential fog above is for, and
	# the two do different jobs.
	#
	# Density is per metre and multiplies against the *length*, which is the trap
	# this walked into twice. Godot's default 0.01 is tuned for the default 64 m
	# volume; at 1,024 m the same figure is sixteen optical depths and the frame
	# comes back as a white sheet with the HUD on it. The useful figure here is
	# two orders of magnitude smaller.
	env.volumetric_fog_enabled = true
	env.volumetric_fog_density = 0.00015
	env.volumetric_fog_length = 1024.0
	env.volumetric_fog_gi_inject = 0.6
	env.volumetric_fog_albedo = _look.fog_colour
	# Sun glint off water and metal. Subtle on purpose: this is a planning game
	# in daylight, not a bloom demo.
	env.glow_enabled = true
	env.glow_intensity = 0.5
	env.glow_bloom = 0.05
	env.glow_hdr_threshold = 1.1
	env.glow_blend_mode = Environment.GLOW_BLEND_MODE_SOFTLIGHT
	env.adjustment_enabled = true
	env.adjustment_brightness = _look.grade_brightness
	env.adjustment_contrast = _look.grade_contrast
	env.adjustment_saturation = _look.grade_saturation

	$Sky.environment = env
	# Held so the day/night pass can move exposure and ambient without rebuilding
	# the whole environment sixty times a second.
	_environment = env
	_sun_set_once = false

	# Physical exposure, which is what makes the tonemapper's numbers mean
	# something rather than being a multiplier somebody tuned by eye.
	var attrs := CameraAttributesPractical.new()
	attrs.auto_exposure_enabled = false
	$Sky.camera_attributes = attrs


func _build_terrain() -> void:
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, republic.terrain_surface())
	terrain_node.mesh = mesh

	# The woods, from the same arrays. Forest was a colour on the ground and
	# nothing else until now, which is the largest single thing between this and
	# a game that looks real -- and the reason bare forest floor read as desert
	# the moment the flat green was replaced with a photograph of leaf litter.
	var planted := Forest.plant(forest_node, republic, Forest.species_meshes())
	if OS.is_debug_build():
		print("planted %d trees" % planted)

	# Vertex colour carries the surface kind as a one-hot channel -- red grass,
	# green forest, blue rock, black water -- and terrain.gdshader decides what
	# each looks like. Keeping that decision there rather than in Rust is what
	# lets the art direction change without recompiling the simulation's
	# renderer, and it is why swapping flat colours for photographed materials
	# needed no change on the Rust side at all.
	var mat := ShaderMaterial.new()
	mat.shader = load("res://terrain.gdshader")
	for surface in ["grass", "forest", "rock", "dirt", "shore"]:
		mat.set_shader_parameter("%s_colour" % surface, _surface_texture(surface, "colour"))
		mat.set_shader_parameter("%s_normal" % surface, _surface_texture(surface, "normal"))
		mat.set_shader_parameter("%s_rough" % surface, _surface_texture(surface, "roughness"))
	mat.set_shader_parameter("snow_colour", _surface_texture("snow", "colour"))
	mat.set_shader_parameter("snow_normal", _surface_texture("snow", "normal"))
	mat.set_shader_parameter("water_colour", _look.water)
	mat.set_shader_parameter("contour_strength", _look.contour_strength)
	mat.set_shader_parameter("map_extent", _extent)
	_terrain_material = mat
	terrain_node.material_override = mat


## One CC0 surface map, imported by Godot from `godot/art/textures/`.
##
## The colour maps are sRGB and the normal and roughness maps are not, and
## getting that wrong is quiet: an sRGB-decoded normal map still looks like a
## normal map, it just lights everything at the wrong angle. Godot's importer
## decides from the `.import` file beside each texture, which is why those are
## committed alongside the images.
func _surface_texture(surface: String, map: String) -> Texture2D:
	return load("res://art/textures/%s_%s.jpg" % [surface, map])


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

	# **A lorry, not a brick.** Every vehicle in the republic was one `BoxMesh`
	# 6.0 x 3.0 x 2.5 m in a flat colour -- a lorry, a coach, a snow plough and a
	# recovery truck all the same grey block. At the zoom freight is actually
	# watched from, the thing that says what a vehicle is is its outline, and
	# there was none to read.
	#
	# `tools/build_vehicles.py` makes the bodies and `vehicle_art.gd` says which
	# kind wears which. `marshal::vehicle_positions` carries the kind and the
	# heading alongside the position, so a coach is a coach and a lorry points
	# the way it is going -- it sent bare xyz triples until this commit, which is
	# why every vehicle was one box facing north.
	# One `MultiMesh` per body, the same shape the buildings use. `vehicles_node`
	# keeps its own (unused) multimesh empty and carries the bodies as children,
	# so nothing that reaches for the node by name breaks.
	VehicleArt.check(republic.vehicle_kind_names())
	var bodies := VehicleArt.bodies()
	_vehicle_body_of.clear()
	_vehicle_nodes.clear()
	var order: Array[String] = []
	for name in republic.vehicle_kind_names():
		var body: String = bodies.get(String(name), VehicleArt.LORRY)
		if not order.has(body):
			order.append(body)
		_vehicle_body_of.append(order.find(body))
	for body in order:
		var node := MultiMeshInstance3D.new()
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = _vehicle_mesh(body)
		node.multimesh = mm
		node.name = "Body_%s" % body
		vehicles_node.add_child(node)
		_vehicle_nodes.append(node)

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
	# `[x, y, z, kind, heading]` per vehicle. Sorted into one buffer per body,
	# because a `MultiMesh` draws one mesh and a fleet is thirteen kinds.
	var flat: PackedFloat32Array = republic.vehicle_positions()
	var stride := 5
	var count := flat.size() / stride
	var per_body: Array[Array] = []
	for _i in _vehicle_nodes.size():
		per_body.append([])
	for i in count:
		var o := i * stride
		var kind := int(flat[o + 3])
		var slot: int = _vehicle_body_of[kind] if kind < _vehicle_body_of.size() else 0
		if slot < per_body.size():
			per_body[slot].append(o)

	for slot in _vehicle_nodes.size():
		var mm: MultiMesh = _vehicle_nodes[slot].multimesh
		var rows: Array = per_body[slot]
		if mm.instance_count != rows.size():
			mm.instance_count = rows.size()
		for n in rows.size():
			var o: int = rows[n]
			# The bodies are modelled nose along +X and standing on z=0, so the
			# wheels meet the road without the 1.5 m lift the old box needed to
			# stop it being buried to its middle.
			var at := Vector3(flat[o], flat[o + 1], flat[o + 2])
			var basis := Basis(Vector3.UP, flat[o + 4])
			mm.set_instance_transform(n, Transform3D(basis, at))


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
	var stride := 8
	var count := flat.size() / stride
	# **The count is not enough on its own any more.** Lamps go out when the
	# grid runs short, which changes nothing about how many segments there are —
	# so a signature over the lamp column rides along, and a town going dark
	# redraws.
	var signature := 0
	var k := stride - 1
	while k < flat.size():
		signature = signature * 3 + int(flat[k])
		k += stride
	var stamp := count * 7919 + signature
	if stamp == _roads_shown:
		return
	_roads_shown = stamp
	var mesh := ImmediateMesh.new()
	var lamps: Array[Vector3] = []
	var burning: Array[bool] = []
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
			var lit := int(flat[o + 7])
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
			if lit > 0:
				# Kerbed street, a shade paler than the tarmac beside it, and
				# lamp posts down one side. Placed by the renderer rather than
				# by the player: a lit road is a variant of the road, not four
				# hundred things to site.
				tone = tone.lerp(Color(0.46, 0.46, 0.45), 0.35)
				var run := along.length()
				var spacing := 34.0
				var posts := maxi(1, int(run / spacing))
				for post in posts:
					var f := (float(post) + 0.5) / float(posts)
					lamps.append(a.lerp(b, f) + side.normalized() * (half + 1.4))
					burning.append(lit > 1)
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
	_draw_lamps(mesh, lamps, burning)
	roads_node.mesh = mesh


## Ways that have been ordered and are not drivable yet.
##
## **The HUD could say there were three and never where.** A road site routes
## nothing -- a lorry will not touch it until the last builder-day is paid -- so
## a player who ordered a road and sees no road on the map has no way to tell an
## order that is being worked from an order that was refused, or from one whose
## crews never arrived. Utility sites have been drawn since the grid landed; this
## is the same fact about the other network, and it was the one missing.
##
## Its own mesh rather than a surface on the roads, because the two change on
## completely different schedules: a road segment moves when one is built or a
## lamp goes out, and a site's progress moves every tick a builder works on it.
## Sharing a mesh would mean rebuilding every road in the republic to advance one
## bar.
func _refresh_road_sites() -> void:
	var flat: PackedFloat32Array = republic.road_sites()
	var stride := 6
	@warning_ignore("integer_division")
	var count: int = flat.size() / stride
	# **Progress is in the stamp, and quantised.** Unlike a road, a site changes
	# every tick a builder is on it, so a count alone would freeze the bar and a
	# raw float would rebuild sixty times a second to move it a hair. Two
	# hundredths is finer than the eye reads at map height and coarser than a
	# day's work on anything worth ordering.
	var stamp := count * 7919
	for i in count:
		stamp = stamp * 211 + int(clampf(flat[i * stride + 4], 0.0, 1.0) * 200.0)
	if stamp == _sites_shown:
		return
	_sites_shown = stamp
	if count == 0:
		road_sites_node.mesh = null
		return
	var mesh := ImmediateMesh.new()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, mat)
	for i in count:
		var o := i * stride
		var a := Vector3(flat[o], 0.0, flat[o + 1])
		var b := Vector3(flat[o + 2], 0.0, flat[o + 3])
		var progress: float = clampf(flat[o + 4], 0.0, 1.0)
		var kph: float = flat[o + 5]
		# The width the finished road will have, from the same limit the built
		# ones read. A site that drew at one width would misreport what is coming.
		var half: float = lerpf(2.6, 4.4, clampf((kph - 25.0) / 35.0, 0.0, 1.0))
		# **Drawn as the road being made, from the near end**: the share that is
		# paid for in the road's own colour, and the rest pegged out in survey
		# ochre at the same width, so the shape of the finished way is legible
		# before any of it exists.
		#
		# The two differ by *colour* and not by width, which is a correction made
		# off a rendered frame. The first version drew the unbuilt stretch half
		# as wide and a quarter as strong -- and at the height this camera
		# actually sits at, a road is one pixel across, so half a road is half a
		# pixel and the whole distinction vanished into the anti-aliasing. Both
		# halves were being drawn perfectly and the picture said nothing.
		var done: Vector3 = a.lerp(b, progress)
		if progress > 0.001:
			_stepped_span(mesh, a, done, Color(_look.road_dirt, 0.95), half, 0.45)
		if progress < 0.999:
			_stepped_span(mesh, done, b, PEGGED_OUT, half, 0.45)
	mesh.surface_end()
	road_sites_node.mesh = mesh


## A ribbon between two points that follows the ground under it.
##
## `_span` samples the height at each end only, which is right for the short
## segments a built network is already cut into and wrong for a site: an ordered
## road is one span of any length, and a flat quad over a valley either floats
## over it or disappears into the hillside. Stepped at the same 40 m the laying
## ghost uses, for the same reason -- what the player is asking is whether the
## line goes where they think it does.
func _stepped_span(
	mesh: ImmediateMesh, a: Vector3, b: Vector3, tone: Color, half: float, lift: float
) -> void:
	var along := b - a
	along.y = 0.0
	if along.length() < 0.5:
		return
	var steps: int = clampi(int(along.length() / 40.0), 1, 64)
	for i in steps:
		var p := a.lerp(b, float(i) / float(steps))
		var q := a.lerp(b, float(i + 1) / float(steps))
		_span(mesh, p.x, p.z, q.x, q.z, tone, half, lift)


## Lamp posts, as a second surface on the road mesh.
##
## **Emissive geometry rather than `OmniLight3D` nodes**, and that is a measured
## decision rather than a shortcut: a town has hundreds of these, and hundreds of
## shadow-casting point lights is the one thing in the night work that could
## break the frame target outright. A glowing head over a lit street reads
## correctly at the distance this camera actually sits at, and it costs two
## triangles.
func _draw_lamps(mesh: ImmediateMesh, at: Array[Vector3], burning: Array[bool]) -> void:
	if at.is_empty():
		return
	var post_mat := StandardMaterial3D.new()
	post_mat.vertex_color_use_as_albedo = true
	post_mat.roughness = 0.7
	post_mat.emission_enabled = true
	post_mat.emission = Color(1.0, 0.86, 0.62)
	# Scaled by vertex colour: a dark lamp writes black here and glows not at
	# all, which is what lets one material serve both states.
	post_mat.emission_operator = BaseMaterial3D.EMISSION_OP_MULTIPLY
	mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, post_mat)
	for i in at.size():
		var base := at[i]
		var head := base + Vector3(0.0, 7.0, 0.0)
		var tone := Color(0.34, 0.34, 0.33)
		var glow := Color(1.0, 0.88, 0.66) if burning[i] else Color(0.20, 0.20, 0.21)
		# The column, as a thin cross of two quads so it reads from any bearing.
		# Typed, because an untyped Array literal yields Variant and every
		# expression built from one stops being inferrable — the same parse
		# error the Republic node was causing, from a different direction.
		for axis: Vector3 in [Vector3(0.42, 0.0, 0.0), Vector3(0.0, 0.0, 0.42)]:
			var p0 := base - axis
			var p1 := base + axis
			var p2 := head + axis
			var p3 := head - axis
			for v in [p0, p1, p2, p0, p2, p3]:
				mesh.surface_set_color(tone)
				mesh.surface_add_vertex(v)
		# The lantern: a small horizontal quad that carries the glow.
		var r := 0.9
		var q0 := head + Vector3(-r, 0.0, -r)
		var q1 := head + Vector3(r, 0.0, -r)
		var q2 := head + Vector3(r, 0.0, r)
		var q3 := head + Vector3(-r, 0.0, r)
		for v in [q0, q3, q2, q0, q2, q1]:
			mesh.surface_set_color(glow)
			mesh.surface_add_vertex(v)
	mesh.surface_end()


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


## In the order `Utility::ALL` declares: power, heat, conveyor, pipeline,
## trolley wire.
##
## Five distinct hues rather than a ramp, because these are categories and not a
## quantity -- a player has to be able to tell a belt from a pipe at a glance,
## and two shades of the same colour would read as "more of the same thing".
##
## **Checked against the roster on load rather than trusted.** `Utility::ALL`
## went from two to five while this list stayed at four, and the fifth drew in
## the fallback white -- which looks like a rendering choice rather than like a
## missing row, and is exactly the silent fallback the project's own rules warn
## about. `_check_lines` is what makes the next one fail loudly.
## A stretch of way that has been ordered and not yet made.
##
## Scraped clay, at the width the finished road will have. It reads as a line
## pegged out across the ground rather than as a pale road, which is the whole
## job: a player who ordered a road and sees nothing has no way to tell work in
## hand from an order that was refused.
##
## **Clay rather than the survey ochre this was first drawn in.** Ochre put it
## within a shade of `UTILITY_TONES`' power line, and a pegged-out road that
## looks like a wire is worse than no marking at all. Every other span on the
## map has a hue of its own and this one has to as well — and being kin to the
## dirt road it is about to become is the right kinship for it to have.
const PEGGED_OUT := Color(0.74, 0.54, 0.30, 0.55)


const UTILITY_TONES := [
	Color(0.86, 0.80, 0.42),  # power: overhead line
	Color(0.78, 0.42, 0.32),  # heat: hot main
	Color(0.55, 0.60, 0.66),  # conveyor: steel belt
	Color(0.44, 0.56, 0.44),  # pipeline: painted pipe
	Color(0.62, 0.56, 0.72),  # trolley wire: thin overhead
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


## Keep the open building's figures live.
##
## Per frame rather than on a day boundary, because half of what this panel shows
## moves within a tick — a site's builder-days, a yard filling as a lorry arrives,
## the staff a labour pass just posted. The panel writes labels rather than
## building them, which is the 165x that makes it affordable, and it does nothing
## at all when nothing is selected.
func _refresh_inspector() -> void:
	if inspector.is_open():
		inspector.refresh()
	# The vehicle panel wants it more than the building one does: a lorry moves
	# every frame, and its distance-left, its leg and its bogging odds all move
	# with it.
	if vehicle_panel.is_open():
		vehicle_panel.refresh()


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

	# Winter on the ground rather than only in a number. `Ground::snow` has
	# accumulated and melted since the ground model landed, and until now the
	# only things that read it were the snow overlay and the ploughs -- so a
	# republic under half a metre of snow was rendered in high summer green.
	# Same rule as the boilers: the state decides, never the calendar.
	#
	# Daily, on the same event as the overlay, because snow does not change
	# between frames and a shader parameter set sixty times a second to the same
	# value is the per-tick cost this whole boundary is designed to avoid.
	var winter: PackedFloat32Array = republic.snow()
	if winter.size() > 0:
		_terrain_material.set_shader_parameter("snow_amount", winter[0])

	Overlays.apply(_terrain_material, republic, _overlay, _extent)
	if _overlay == Overlays.Mode.SURVEY:
		survey_node.mesh = Overlays.survey_mesh(republic, _ground_height)
	else:
		survey_node.mesh = null


## Where the cursor is pointing on the ground, in world metres.
##
## The terrain has no collision body — it is one `ArrayMesh` and giving a
## million triangles a collider to answer a mouse position would be absurd. So
## this intersects the camera ray with a flat plane, samples the real ground
## height there, and repeats: three passes converge to well under a metre on
## terrain this gentle, which is finer than the 10 m cells anything is placed on.
func _cursor_ground() -> Vector3:
	var camera: Camera3D = rig.camera
	var mouse := get_viewport().get_mouse_position()
	var from := camera.project_ray_origin(mouse)
	var dir := camera.project_ray_normal(mouse)
	var height := 0.0
	var hit := Vector3.ZERO
	for _pass in 3:
		# Looking up, or along the horizon: nothing to hit.
		if absf(dir.y) < 0.0001:
			return hit
		var t := (height - from.y) / dir.y
		if t < 0.0:
			return hit
		hit = from + dir * t
		height = _ground_height(hit.x, hit.z)
	hit.y = height
	return hit


## Open a panel on whatever is under the cursor, or close both.
##
## **Asks the simulation what is there rather than testing the drawn geometry.**
## `building_at` runs the same footprint rule placement refuses to overlap, so a
## building that is visibly standing somewhere is always selectable there — a hit
## test written here against the `MultiMesh` transforms would be a second copy of
## that rule, and it is the copy that drifts. `vehicle_at` answers from the same
## positions the renderer draws from, for the same reason.
##
## **Vehicles are tried first, and that is the right way round.** A lorry parked
## in its garage's yard stands inside a footprint, so a building-first pick would
## make every parked vehicle in the republic unclickable — and the parked ones
## are exactly the ones a player wants to ask about. A moving lorry is never
## inside a footprint, so the order costs a building click nothing.
##
## Clicking open ground closes whatever is up, which is what a player means by it.
func _pick_building() -> void:
	var at := _cursor_ground()
	var vehicle := republic.vehicle_at(at.x, at.z, vehicle_panel.PICK_RADIUS)
	if vehicle >= 0:
		inspector.close_panel()
		vehicle_panel.open(republic, vehicle)
		audio.play("click")
		return
	vehicle_panel.close_panel()
	var id := republic.building_at(at.x, at.z)
	if id <= 0:
		inspector.close_panel()
		return
	inspector.open(republic, _store, id)
	audio.play("click")


## Start placing a building the build menu chose.
func _begin_placement(kind: int, market: int) -> void:
	_build_kind = kind
	_build_market = market
	_set_screen(Screen.PLAYING)
	if _ghost == null:
		_ghost = MeshInstance3D.new()
		var mat := StandardMaterial3D.new()
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.albedo_color = Color(0.4, 0.9, 0.45, 0.45)
		# Unshaded, so the ghost reads as an overlay rather than as a building
		# that is already there and lit like its neighbours.
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		_ghost.material_override = mat
		add_child(_ghost)
	var size: Vector2 = republic.building_kind_size(kind)
	var box := BoxMesh.new()
	box.size = Vector3(size.x, 8.0, size.y)
	_ghost.mesh = box
	_ghost.visible = true


func _cancel_placement() -> void:
	_build_kind = -1
	_build_market = -1
	if _ghost != null:
		_ghost.visible = false


## Move the ghost and colour it by whether the placement would be accepted.
##
## The preview asks `can_place`, which is the same rule the commit uses — a
## preview that asked a different question would render green over ground the
## placement then refuses, which is worse than no preview.
func _update_ghost() -> void:
	if _build_kind < 0 or _ghost == null:
		return
	var at := _cursor_ground()
	_ghost.position = Vector3(at.x, at.y + 4.0, at.z)
	var why := String(republic.can_place(_build_kind, at.x, at.z))
	var mat: StandardMaterial3D = _ghost.material_override
	mat.albedo_color = (
		Color(0.4, 0.9, 0.45, 0.45) if why == "" else Color(0.9, 0.35, 0.3, 0.45)
	)
	hud.set_placing(
		String(republic.building_kind_name(_build_kind)), why, _ground_note(at)
	)


## What the ground under the cursor is like, while something is in hand.
##
## **Siting is the decision this game is made of and these were all unaskable.**
## Every figure here is a placement query the simulation already answered for
## anybody who called it, and nothing did: how hard the going is decides whether a
## lorry gets through in spring, how far the frontier is decides the haul to a
## customs house, which bloc is nearest decides which currency this corner earns,
## and what a visitor would think of the view decides whether a hotel is worth
## building at all.
##
## The appeal figure appears only where there is any, which on a map with nothing
## worth looking at is nowhere. A figure that is always on screen and matters
## twice a republic is a figure a player learns to stop reading.
func _ground_note(at: Vector3) -> String:
	var going: float = republic.going_at(at.x, at.z)
	var border: float = republic.distance_to_border(at.x, at.z)
	var bloc: int = republic.bloc_near(at.x, at.z)
	var parts := PackedStringArray()
	# Going is a badness -- 0 firm, 1 impassable -- and it is stated as one, in
	# the direction the simulation stores it. The overlay ramp was built off a
	# comment that had this backwards once and painted a dry map entirely red.
	parts.append("going %d%%" % int(round(going * 100.0)))
	parts.append("%.0f m from the %s frontier" % [
		border, "eastern" if bloc == 0 else "western",
	])
	var appeal: float = republic.appeal_at(at.x, at.z)
	if appeal > 0.005:
		parts.append("appeal %d%%" % int(round(appeal * 100.0)))
	return "  ·  ".join(parts)


## Commit the placement under the cursor.
func _commit_placement() -> void:
	var at := _cursor_ground()
	var why := ""
	if _build_market < 0:
		why = String(republic.place(_build_kind, at.x, at.z))
	else:
		why = String(republic.contract(_build_kind, at.x, at.z, _build_market))
	if why != "":
		hud.set_placing(String(republic.building_kind_name(_build_kind)), why)
		return
	# Stay in placement mode: putting up a row of houses should not mean six
	# trips back to the menu.
	_buildings_shown = -1
	hud.set_placing(String(republic.building_kind_name(_build_kind)), "")


## Start laying a way: click once for each end.
##
## **A road was as unbuildable as a building was before the build menu existed.**
## `order_way` has been on the shell since M6 and no `.gd` file ever called it,
## which on a map that now starts empty means a republic could put up a factory
## and never connect it to anything.
func _begin_way(grade: int, lamps: bool) -> void:
	_begin_span()
	_span_grade = grade
	_span_lamps = lamps


## Start stringing a line: the same two clicks, for the other network.
##
## **`order_line` was in exactly the state `order_way` had been in.** A republic
## could raise a power station and had no way to run a wire out of it, so every
## building beyond a plant's own reach was dark for ever and the whole utility
## module -- ordered spans, a bill of materials per kilometre, a crew that builds
## them, loss over the length of the network -- was reachable from the simulation
## and from nowhere a player goes.
##
## The same tool rather than a second one, because it is the same act: two points
## on the ground and something laid between them. What differs is the command it
## ends in and how the band is drawn.
func _begin_line(kind: int) -> void:
	_begin_span()
	_span_line = kind


## The half the two share: put down whatever is in hand and raise the band.
func _begin_span() -> void:
	_cancel_placement()
	_cancel_span()
	_set_screen(Screen.PLAYING)
	if _span_ghost == null:
		_span_ghost = MeshInstance3D.new()
		var mat := StandardMaterial3D.new()
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.vertex_color_use_as_albedo = true
		_span_ghost.material_override = mat
		add_child(_span_ghost)
	_span_ghost.visible = true


## Is there a span in hand at all? Either kind counts.
func _laying() -> bool:
	return _span_grade >= 0 or _span_line >= 0


func _cancel_span() -> void:
	_span_grade = -1
	_span_line = -1
	_span_started = false
	if _span_ghost != null:
		_span_ghost.visible = false
	hud.set_placing("", "")


## First click sets the near end, second orders the span and starts the next one
## from where this ended -- so a chain of road, or of wire, is a chain of clicks.
func _click_span() -> void:
	var at := _cursor_ground()
	if not _span_started:
		_span_from = at
		_span_started = true
		return
	var why := ""
	if _span_line >= 0:
		why = String(
			republic.order_line(_span_line, _span_from.x, _span_from.z, at.x, at.z)
		)
	else:
		why = String(
			republic.order_way(_span_grade, _span_from.x, _span_from.z, at.x, at.z, _span_lamps)
		)
	hud.set_placing(_span_label(), why)
	if why == "":
		# A line site is drawn off `_lines_shown` and a way site off
		# `_sites_shown`; both count what the simulation reports, so both notice
		# the new order on their own. The road mesh does not -- a promoted track
		# is the only thing that changes it -- so it is told.
		_roads_shown = -1
		_span_from = at


func _span_label() -> String:
	if _span_line >= 0:
		var lines := republic.utility_names()
		return String(lines[_span_line]) if _span_line < lines.size() else "line"
	var names := republic.grade_names()
	var name := String(names[_span_grade]) if _span_grade < names.size() else "way"
	return "%s%s" % [name, " with lamps" if _span_lamps else ""]


## The rubber band between the first click and the cursor.
##
## Drawn rather than described, because the whole question a player is asking
## while laying a road is "does this line go where I think it does" -- and on
## ground with relief in it, an unlit line at a fixed height would answer wrongly.
func _update_span() -> void:
	if not _laying() or _span_ghost == null:
		return
	var at := _cursor_ground()
	if not _span_started:
		hud.set_placing(_span_label(), "click the near end")
		_span_ghost.mesh = null
		return
	var mesh := ImmediateMesh.new()
	var along := at - _span_from
	along.y = 0.0
	if along.length() > 0.5:
		# A wire is not a road, and the band says so before the first click
		# lands: narrower, higher off the ground, and in the tone the strung
		# spans are already drawn in rather than the road green.
		var wire := _span_line >= 0
		var side := along.normalized().cross(Vector3.UP).normalized() * (1.6 if wire else 3.5)
		var lift := Vector3(0.0, 6.0 if wire else 0.6, 0.0)
		var tone := _utility_tone(_span_line) if wire else Color(0.45, 0.85, 0.5, 0.5)
		tone.a = 0.5
		mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES, null)
		# Stepped along the ground rather than a single quad, so the band follows
		# the relief it is being drawn over.
		var steps := clampi(int(along.length() / 40.0), 1, 64)
		for i in steps:
			var t0 := float(i) / float(steps)
			var t1 := float(i + 1) / float(steps)
			var a := _span_from.lerp(at, t0)
			var b := _span_from.lerp(at, t1)
			a.y = _ground_height(a.x, a.z)
			b.y = _ground_height(b.x, b.z)
			var p0 := a - side + lift
			var p1 := a + side + lift
			var p2 := b + side + lift
			var p3 := b - side + lift
			for v in [p0, p3, p2, p0, p2, p1]:
				mesh.surface_set_color(tone)
				mesh.surface_add_vertex(v)
		mesh.surface_end()
	_span_ghost.mesh = mesh
	hud.set_placing(
		_span_label(),
		"%.0f m  ·  click the far end, right click or esc to stop" % _span_from.distance_to(at)
	)


func _ground_height(x: float, z: float) -> float:
	# The terrain mesh is the ground, so a disc drawn on it has to sit on the
	# same surface rather than on a plane at zero.
	return republic.ground_height(x, z)
