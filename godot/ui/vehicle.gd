extends CanvasLayer

## One vehicle, in front of the player: what it is carrying, where to, and what
## is going to happen to it on the way.
##
## **The lorry was the last thing on the map you could not ask anything.** A
## building has an inspector, a crew has a marker, a road has a colour that says
## what it is; a vehicle had a silhouette and a heading. Meanwhile the simulation
## knew what was in the bed, which yard it was bound for, how much of the journey
## was left, whether the tank would cover it, how likely it was to stick on the
## crossing it was about to make, and whether there would be room when it got
## there. Every deadlocked republic is a freight question and freight was the
## half of the game with no panel.
##
## # The two figures this exists for
##
## `bog_chance` and `vehicle_destination_fullness` are the reason there is a
## screen here rather than a line in the HUD.
##
## **Bogging is the one deliberately random mechanic in the game.** A model that
## takes a lorry away on a die roll owes the player the odds *before* the roll,
## or it reads as the game being arbitrary rather than as the ground being soft.
## The simulation computes them exactly and would have kept them to itself.
##
## **A full yard is a wasted journey nobody could see coming.** A lorry hauling
## coal to a store with no room in it drives the whole way and comes back laden,
## and until this panel the only evidence was a lorry that kept setting off and
## achieving nothing.
##
## # A panel, not a sheet
##
## The same shape and the same reasoning as the building inspector: this is about
## a thing standing in a field, and a full-screen page would hide the thing it is
## about. The two are mutually exclusive -- `main.gd` closes one when it opens
## the other -- because both live in the same strip along the bottom.
##
## # Nothing here decides anything, and nothing here commands anything
##
## Deliberately read-only. A vehicle in this game is dispatched by the freight
## system out of what the republic's buildings are asking for; there is no
## "send this lorry there" and there should not be one, because that would be
## the player hand-routing around a plan instead of fixing the plan. What the
## panel is for is finding out *why* the plan is doing what it is doing.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")

signal closed

## The layout of `views::vehicle_state`, named once. The building inspector's
## note applies here for the same reason: reading `state[13]` at six call sites
## is how a stride change becomes a panel quietly showing the wrong column.
const KIND := 0
const STATE := 1
const AT_X := 2
const AT_Y := 3
const CAPACITY := 4
const CARRIED := 5
const CARGO := 6
const FUEL := 7
const TANK := 8
const JOB := 9
const JOB_RESOURCE := 10
const JOB_TONNES := 11
const DESTINATION := 12
const DESTINATION_BUILDING := 13
const LEG := 14
const LEGS := 15
const ON_ROAD := 16
const REMAINING := 17
const BOGGED_DAYS := 18

## What a vehicle is doing, in `views::vehicle_state`'s order. Checked against
## the stride on open, the same way the inspector checks its stall words.
const STATE_WORDS := ["parked", "driving out empty", "delivering", "coming home", "bogged"]

## What it was sent to do, in the same order. Seven, because the fleet does seven
## kinds of work -- a wildcard here would have reported a plough as idle.
const JOB_WORDS := [
	"hauling", "recovering a casualty", "carrying builders",
	"collecting a crew", "bringing settlers", "carrying visitors", "ploughing",
]

## Where it is delivering, by `DESTINATION`. A road site and a line site have
## their own numbering and no name, which is why they are words rather than a
## lookup.
const DESTINATION_WORDS := ["a road being laid", "a line being strung"]

## How near a click has to land, in metres. A lorry is about six metres long and
## the camera sits high enough that a pixel is several of them, so the reach is
## generous -- and `main.gd` tries the vehicle pick first only when it hits, so
## a wide radius costs a building click nothing.
const PICK_RADIUS := 30.0

## Above this share of capacity, the place a load is bound for is called full.
## Not a rule about the simulation -- it will still take what fits -- but the
## point at which telling the player is worth the line.
const CROWDED := 0.9

var _republic: Republic = null

## Which vehicle is up, or -1 for none. Unlike a building id, this is an index
## into the fleet and zero is a real vehicle.
var _index := -1
## Which vehicle the body was built for, so a refresh knows when to rebuild.
var _shown := -1

var _panel: PanelContainer = null
var _title: Label = null
var _status: Label = null
var _notice: Label = null
var _body: HBoxContainer = null

## The labels a refresh writes, by name.
var _figures := {}


func _ready() -> void:
	layer = 14
	visible = false
	_build_chrome()


func _build_chrome() -> void:
	_panel = PanelContainer.new()
	_panel.theme_type_variation = "Instrument"
	_panel.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	_panel.offset_left = 410.0
	_panel.offset_right = -312.0
	_panel.offset_bottom = -44.0
	# Shorter than the building inspector: a vehicle has three columns of figures
	# and a building has up to six, and a panel padded out to a fixed height
	# would be an instrument that is mostly empty.
	_panel.offset_top = -216.0
	_panel.grow_vertical = Control.GROW_DIRECTION_BEGIN
	add_child(_panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", P.GAP_TIGHT)
	_panel.add_child(column)

	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", P.GAP_WIDE)
	head.alignment = BoxContainer.ALIGNMENT_CENTER
	column.add_child(head)

	_title = Parts.say("", "Section")
	head.add_child(_title)
	_status = Parts.say("", "Stamp")
	head.add_child(_status)
	head.add_child(Parts.fill())

	var close := Parts.button("CLOSE", "Quiet")
	close.pressed.connect(close_panel)
	head.add_child(close)

	column.add_child(Parts.rule())

	_notice = Parts.say("", "Alarm")
	_notice.custom_minimum_size = Vector2(0, 18)
	_notice.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_notice)

	_body = HBoxContainer.new()
	_body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", P.GAP_SECTION)
	_body.clip_contents = true
	column.add_child(_body)


# ---- opening and closing -----------------------------------------------------


## Show a vehicle by its place in the fleet. A negative index closes the panel.
func open(republic: Republic, index: int) -> void:
	_republic = republic
	if index < 0:
		close_panel()
		return
	_index = index
	visible = true
	refresh()


func close_panel() -> void:
	if not visible:
		return
	visible = false
	_index = -1
	closed.emit()


func is_open() -> bool:
	return visible and _index >= 0


func selected() -> int:
	return _index


# ---- the refresh -------------------------------------------------------------


func refresh() -> void:
	if _republic == null or _index < 0:
		return
	var state: PackedFloat32Array = _republic.vehicle_state(_index)
	# An empty read is the fleet saying this vehicle is gone. Closing is the
	# honest response; leaving the last figures up would be a panel about
	# something that is not there.
	if state.size() < _republic.vehicle_state_stride():
		close_panel()
		return

	if _shown != _index:
		_build_body(state)
		_shown = _index

	_write_head(state)
	_write_figures(state)


func _write_head(state: PackedFloat32Array) -> void:
	var names: PackedStringArray = _republic.vehicle_kind_names()
	var kind := int(state[KIND])
	_title.text = (
		String(names[kind]).to_upper() if kind >= 0 and kind < names.size() else "VEHICLE"
	)

	var doing := int(state[STATE])
	_status.text = (
		STATE_WORDS[doing].to_upper() if doing >= 0 and doing < STATE_WORDS.size() else ""
	)
	# Stuck is the one state that is a problem rather than a description, so it
	# is the one that changes colour. Everything else a lorry does is fine.
	_status.add_theme_color_override(
		"font_color", P.ALARM if doing == 4 else P.OCHRE
	)


func _build_body(_state: PackedFloat32Array) -> void:
	for child in _body.get_children():
		child.queue_free()
	_figures.clear()

	# **Three columns, in the order the questions get asked.** What is in it,
	# where is it going, and what will go wrong -- which is the order a player
	# reads a stopped lorry in.
	var load := _section("The load", 1.0)
	_line(load, "carrying", "in the bed")
	_line(load, "ordered", "sent for")
	_line(load, "fuel", "fuel")

	var run := _section("The run", 1.2)
	_line(run, "doing", "work")
	_line(run, "bound", "bound for")
	_line(run, "left", "distance left")
	_line(run, "surface", "under the wheels")

	var risk := _section("What is against it", 1.2)
	_line(risk, "bog", "sticking, this leg")
	_line(risk, "room", "room where it lands")


func _section(title: String, weight: float = 1.0) -> VBoxContainer:
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.size_flags_stretch_ratio = weight
	box.add_theme_constant_override("separation", 1)
	_body.add_child(box)
	box.add_child(Parts.say(title.to_upper(), "Stamp"))
	box.add_child(Parts.rule())
	return box


func _line(into: VBoxContainer, key: String, label: String) -> Label:
	var line := HBoxContainer.new()
	line.custom_minimum_size = Vector2(0, 19)
	line.add_theme_constant_override("separation", P.GAP)
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	into.add_child(line)

	var name_label := Parts.say(label, "Faint")
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(name_label)

	var value := Parts.figure("")
	value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
	value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	value.size_flags_stretch_ratio = 1.6
	line.add_child(value)
	_figures[key] = value
	return value


func _write(key: String, text: String, tone: Color = P.PAPER) -> void:
	var label: Label = _figures.get(key)
	if label == null:
		return
	label.text = text
	label.add_theme_color_override("font_color", tone)


func _write_figures(state: PackedFloat32Array) -> void:
	var resources: PackedStringArray = _republic.resource_names()

	# ---- the load ----
	var carried: float = state[CARRIED]
	var cargo := int(state[CARGO])
	if carried < 0.005:
		_write("carrying", "empty", P.PAPER_FAINT)
	else:
		var what := (
			String(resources[cargo]).to_lower()
			if cargo >= 0 and cargo < resources.size() else "goods"
		)
		_write("carrying", "%.1f of %.0f t %s" % [carried, state[CAPACITY], what])

	var ordered := int(state[JOB_RESOURCE])
	if ordered >= 0 and ordered < resources.size():
		_write("ordered", "%.0f t %s" % [
			state[JOB_TONNES], String(resources[ordered]).to_lower(),
		])
	elif int(state[JOB]) == 2 or int(state[JOB]) == 4 or int(state[JOB]) == 5:
		# Buses and coaches are sent for people rather than for tonnes, and
		# `JOB_TONNES` carries the head count on a ferry.
		_write("ordered", "%d aboard" % int(state[JOB_TONNES]))
	else:
		_write("ordered", "—", P.PAPER_FAINT)

	# Against the journey rather than as a bare figure. A tank three-quarters
	# full means nothing; a tank that will not reach the far end is the whole
	# reason a lorry is stranded, and it is a thing the player can still fix.
	var fuel: float = state[FUEL]
	var tank: float = state[TANK]
	var share: float = fuel / tank if tank > 0.0 else 0.0
	_write("fuel", "%.2f of %.2f t" % [fuel, tank], P.ALARM if share < 0.15 else P.PAPER)

	# ---- the run ----
	var job := int(state[JOB])
	if job < 0:
		_write("doing", "nothing in hand", P.PAPER_FAINT)
	elif job < JOB_WORDS.size():
		_write("doing", JOB_WORDS[job])
	else:
		# The roster in the simulation has outgrown the words here. Said rather
		# than swallowed: a blank line reads as "no job" and this is the opposite.
		_write("doing", "work this panel has no word for", P.ALARM)

	var destination := int(state[DESTINATION])
	if destination < 0:
		_write("bound", "—", P.PAPER_FAINT)
	elif destination == 0:
		var id := int(state[DESTINATION_BUILDING])
		var at: PackedFloat32Array = _republic.building_state(id)
		if at.size() > 0:
			_write("bound", String(_republic.building_kind_name(int(at[0]))))
		else:
			_write("bound", "a building")
	else:
		_write("bound", DESTINATION_WORDS[mini(destination - 1, DESTINATION_WORDS.size() - 1)])

	var legs := int(state[LEGS])
	if legs <= 0:
		_write("left", "standing still", P.PAPER_FAINT)
	else:
		var metres: float = state[REMAINING]
		_write("left", "%s  ·  leg %d of %d" % [
			"%.0f m" % metres if metres < 1000.0 else "%.1f km" % (metres / 1000.0),
			int(state[LEG]) + 1, legs,
		])

	if legs <= 0:
		_write("surface", "—", P.PAPER_FAINT)
	elif state[ON_ROAD] > 0.5:
		_write("surface", "road", P.GOOD)
	else:
		# Off road is not a fault, it is the republic's road network not being
		# there yet -- which is the single most common reason a young republic's
		# freight is slow, and it is invisible from a map with no road on it.
		_write("surface", "open ground", P.OCHRE)

	# ---- what is against it ----
	var doing := int(state[STATE])
	if doing == 4:
		var days := int(state[BOGGED_DAYS])
		_write("bog", "stuck %d day%s" % [days, "" if days == 1 else "s"], P.ALARM)
	elif legs <= 0:
		_write("bog", "—", P.PAPER_FAINT)
	else:
		# The odds on the leg it is on, from the simulation's own model. Shown
		# before the roll rather than after it, which is the whole argument for
		# a probability being visible at all.
		var odds: float = _republic.bog_chance(_index, int(state[LEG]))
		if odds < 0.005:
			_write("bog", "nothing in it", P.GOOD)
		else:
			_write("bog", "%.0f in a hundred" % (odds * 100.0),
				P.ALARM if odds > 0.2 else P.OCHRE)

	var fullness: float = _republic.vehicle_destination_fullness(_index)
	if fullness < 0.0:
		_write("room", "—", P.PAPER_FAINT)
	elif fullness >= 1.0:
		_write("room", "full — this trip is wasted", P.ALARM)
	elif fullness > CROWDED:
		_write("room", "%.0f%% full" % (fullness * 100.0), P.ALARM)
	else:
		_write("room", "%.0f%% full" % (fullness * 100.0), P.PAPER)


## Prove the panel reaches a real vehicle, and say so. Returns a report line.
##
## The same self-check every other screen carries, and it earns its place the
## same way: `cargo build` proves this file parses and proves nothing at all
## about whether `vehicle_state` hands back a row this panel can read. A stride
## that moved would show as a panel of plausible wrong numbers.
func check(republic: Republic) -> String:
	var count: int = republic.vehicle_count()
	if count <= 0:
		return "vehicle check FAILED: the republic has no vehicles to inspect"
	var stride: int = republic.vehicle_state_stride()
	if stride != BOGGED_DAYS + 1:
		return (
			"vehicle check FAILED: the shell packs %d floats per vehicle and this "
			+ "panel names %d"
		) % [stride, BOGGED_DAYS + 1]
	if JOB_WORDS.size() != republic.vehicle_job_count():
		return (
			"vehicle check FAILED: the fleet does %d kinds of work and this panel "
			+ "has %d words for them"
		) % [republic.vehicle_job_count(), JOB_WORDS.size()]

	var state: PackedFloat32Array = republic.vehicle_state(0)
	if state.size() != stride:
		return "vehicle check FAILED: vehicle 0 read back %d floats, not %d" % [
			state.size(), stride,
		]

	# The pick, through the same binding the click uses, aimed at where the
	# simulation says the vehicle is standing. A panel that cannot be reached by
	# clicking the thing it is about is the gap this whole milestone is closing.
	var picked: int = republic.vehicle_at(state[AT_X], state[AT_Y], PICK_RADIUS)
	if picked < 0:
		return "vehicle check FAILED: nothing was picked where vehicle 0 stands"

	open(republic, picked)
	var read := _figures.size()
	close_panel()
	if read == 0:
		return "vehicle check FAILED: the panel opened and built no figures"
	return "vehicle check ok: picked %d of %d, read %d lines" % [picked, count, read]
