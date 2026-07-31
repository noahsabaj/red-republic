extends CanvasLayer

## The republic's instrument panel.
##
## Every number here is read from a view the simulation already owns. Nothing in
## this file works anything out: the moment a panel starts deriving its own answer
## there are two versions of the balance and only one of them is tested.
##
## # The shape: a strip, a state of the republic, and two returns
##
## **A strip across the top** carries the four things that are true whatever the
## player is doing -- where they are, what day it is, how fast it is running, and
## what the weather is about to do. It is thin because it is always there.
##
## **A panel down the left** is the state of the republic: who there is, what they
## are building, who is arriving and leaving, and what is strung to what. Ruled
## label-and-figure rows under stamped section heads, which is the same table the
## build and labour screens use -- an instrument and a form should not read as two
## different applications.
##
## **Two returns down the right** are the stockpiles and the people, which are the
## two lists long enough to want scrolling.
##
## What this replaced was one opaque block of ten dense sentences over the
## top-left quarter of the map. Every fact on it is still here; none of it is a
## sentence any more, because a sentence cannot be scanned and a column can.
##
## # Long lists are pooled, and that is a measured rule
##
## Building 5,000 `Label` nodes costs 229 ms -- a visible hitch -- while updating
## them costs nothing: ~46 µs per label to create against 0.2 µs per frame to
## refresh, a factor of 165. So the tables build their rows once and reuse them.
## Nothing here creates a node per datum per frame.
##
## # No literal is ever sized against a roster
##
## The pools are sized from the rosters themselves. A literal row count beside a
## roster is a second copy of that roster, and it is the copy that fails silently:
## the table stops one row short, nothing errors, nothing warns, and no test
## covers it because none of it is simulation state. It has happened five times in
## this file -- four row counts and, once, the panel geometry, which nobody had
## thought of as a roster-sized number until two tables were drawn on top of each
## other.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Overlays := preload("res://ui/overlays.gd")

## How tall a row is here. Tighter than a full screen's, because this panel sits
## over the republic and every pixel of it is a pixel of map nobody can see.
const HUD_ROW := 19

## The order `demographics()` hands them back in, which is the order
## `LifeStage::ALL` and `Education::ALL` declare. Named here rather than read
## across the boundary because these are labels, not simulation facts.
const STAGE_NAMES := ["Infants", "Pupils", "Students", "Workers", "Retired"]
const LEARNING_NAMES := ["Unschooled", "Schooled", "Graduates"]

## The rows of the left-hand panel, in order: `[section, key, label]`. A section
## of `""` continues the one above.
##
## A table rather than a run of `_add_row` calls, so the panel's contents are one
## list to read and `refresh` can write to it by name.
const STATE_ROWS := [
	["The republic", "people", "people"],
	["", "standing", "standing"],
	["", "treasury", "treasury"],
	["", "content", "contentment"],
	["", "wellbeing", "health / loyalty"],
	["Construction", "builders", "builders"],
	["", "hired", "hired abroad"],
	["Movement", "settlers", "settlers"],
	["", "waiting", "at the frontier"],
	["", "visitors", "visitors"],
	["Networks", "ways", "ways"],
	["", "lit", "street lighting"],
	["", "grid", "power & heat"],
	["", "unserved", "not served"],
	["", "imports", "imports"],
	["Weather", "ahead", "five days"],
	["", "snow", "snow"],
]

var _stock_rows := 0
var _people_rows := 0
var _stock_labels: Array[Label] = []
var _people_labels: Array[Label] = []
var _stock_grid: GridContainer = null
var _people_grid: GridContainer = null

var _clock: Label = null
var _place: Label = null
var _speed: Label = null
var _weather: Label = null
var _state := {}
var _hint: Label = null

## The three things that compete for the one line along the bottom. Held apart
## rather than written over each other, because a placement refusal must not cost
## the player the key list and the overlay must not cost them the refusal.
var _hint_overlay := ""
var _hint_keys := ""
var _hint_placing := ""

var _resource_names := PackedStringArray()
var _resource_forms := PackedStringArray()
var _content_names := PackedStringArray()
var _utility_names := PackedStringArray()
var _way_names := PackedStringArray()


func _ready() -> void:
	_build_strip()
	_build_state_panel()
	_build_returns()
	_build_hint()


# ---- the chrome --------------------------------------------------------------


## The strip across the top: where, when, how fast, and what the sky is doing.
func _build_strip() -> void:
	var panel := PanelContainer.new()
	panel.theme_type_variation = "Instrument"
	panel.set_anchors_preset(Control.PRESET_TOP_WIDE)
	panel.offset_left = 12.0
	panel.offset_right = -12.0
	panel.offset_top = 10.0
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(panel)

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", P.GAP_SECTION)
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	panel.add_child(line)

	# The republic's own name leads, because it is the one thing on this panel the
	# player wrote themselves, and a game that forgets what you called your
	# republic is a game that has not noticed you were there.
	_place = Parts.say("", "Section")
	line.add_child(_place)
	_clock = Parts.say("", "Figure")
	line.add_child(_clock)
	_speed = Parts.say("", "Stamp")
	line.add_child(_speed)
	line.add_child(Parts.fill())
	# **Temperature only.** The forecast and the snow live in the panel's WEATHER
	# block, and a strip that repeated them would be the same three figures twice
	# on one screen -- which is how a player learns to stop reading one of them.
	# What earns a place up here is the one number that is true at a glance.
	_weather = Parts.say("", "Figure")
	_weather.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	line.add_child(_weather)


## The state of the republic, down the left.
func _build_state_panel() -> void:
	var panel := PanelContainer.new()
	panel.theme_type_variation = "Instrument"
	panel.set_anchors_preset(Control.PRESET_TOP_LEFT)
	panel.offset_left = 12.0
	panel.offset_top = 58.0
	panel.custom_minimum_size = Vector2(390, 0)
	add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	panel.add_child(column)

	var section := ""
	for spec in STATE_ROWS:
		if String(spec[0]) != "":
			if section != "":
				column.add_child(Parts.gap(P.GAP_TIGHT))
			section = String(spec[0])
			column.add_child(Parts.say(section.to_upper(), "Stamp"))
			column.add_child(Parts.rule())
		_state[String(spec[1])] = _state_row(column, String(spec[2]))


## One label-and-figure line of the left-hand panel.
func _state_row(into: VBoxContainer, label: String) -> Label:
	var line := HBoxContainer.new()
	line.custom_minimum_size = Vector2(0, HUD_ROW)
	line.add_theme_constant_override("separation", P.GAP)
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	into.add_child(line)

	var name_label := Parts.say(label, "Faint")
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.size_flags_stretch_ratio = 1.0
	line.add_child(name_label)

	var value := Parts.figure("")
	value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
	value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	value.size_flags_stretch_ratio = 2.0
	line.add_child(value)
	return value


## The two long lists, down the right.
##
## They stack in a container rather than sitting at literal offsets, and that is a
## bug fix rather than tidying: a `PanelContainer` grows to fit its contents, so
## the moment the resource roster passed about twenty-two rows the stockpile table
## grew straight through the gap under it and the two were drawn on top of each
## other. A container makes overlap structurally impossible, because neither panel
## can grow into the other.
func _build_returns() -> void:
	var side := VBoxContainer.new()
	side.set_anchors_preset(Control.PRESET_RIGHT_WIDE)
	side.offset_left = -300.0
	side.offset_right = -12.0
	side.offset_top = 58.0
	side.offset_bottom = -44.0
	side.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	side.add_theme_constant_override("separation", P.GAP)
	add_child(side)

	_stock_grid = _return_panel(side, "Stockpiles")
	_people_grid = _return_panel(side, "The people")


## One scrolling return: a stamped head and a two-column grid under it.
func _return_panel(into: VBoxContainer, title: String) -> GridContainer:
	var panel := PanelContainer.new()
	panel.theme_type_variation = "Instrument"
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	into.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", P.GAP_TIGHT)
	panel.add_child(column)
	column.add_child(Parts.say(title.to_upper(), "Stamp"))
	column.add_child(Parts.rule())

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	column.add_child(scroll)

	var grid := GridContainer.new()
	grid.columns = 2
	grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	grid.add_theme_constant_override("h_separation", P.GAP_WIDE)
	grid.add_theme_constant_override("v_separation", 1)
	scroll.add_child(grid)
	return grid


func _build_hint() -> void:
	_hint = Parts.say("", "Faint")
	_hint.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	_hint.offset_left = 16.0
	_hint.offset_top = -32.0
	_hint.grow_vertical = Control.GROW_DIRECTION_BEGIN
	add_child(_hint)


# ---- the pooled tables -------------------------------------------------------


func set_resource_names(names: PackedStringArray, forms: PackedStringArray) -> void:
	_resource_names = names
	_resource_forms = forms
	_build_pool(_stock_grid, _stock_labels, names.size())
	_stock_rows = names.size()


func set_contentment_names(names: PackedStringArray) -> void:
	_content_names = names
	# Three headings, the life stages, the education levels, the components, and
	# the comfort lift -- which is a row of its own because it is not one of the
	# components. Counted rather than given slack: slack is a literal that has not
	# been exceeded yet, and that is exactly how this table lost a row four times
	# running.
	var rows := 3 + STAGE_NAMES.size() + LEARNING_NAMES.size() + names.size() + 1
	_build_pool(_people_grid, _people_labels, rows)
	_people_rows = rows


func set_utility_names(names: PackedStringArray) -> void:
	_utility_names = names


func set_way_names(names: PackedStringArray) -> void:
	_way_names = names


## Build a table's rows once. See the pooling note at the top of this file.
func _build_pool(grid: GridContainer, pool: Array[Label], rows: int) -> void:
	if grid == null or rows <= pool.size() / 2:
		return
	for label in pool:
		label.queue_free()
	pool.clear()
	for i in rows:
		var name_label := Parts.say("", "Small")
		name_label.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
		grid.add_child(name_label)
		pool.append(name_label)

		var value := Parts.figure("")
		value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
		value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		grid.add_child(value)
		pool.append(value)


# ---- the refresh -------------------------------------------------------------


func refresh(republic: Republic, overlay_mode: int, speed_names: Array) -> void:
	var title: String = republic.republic_name()
	_place.text = title.to_upper() if title != "" else "UNNAMED"
	_clock.text = String(republic.date_text())
	_speed.text = String(speed_names[republic.speed()]).to_upper()
	_refresh_weather(republic)

	_write("people", "%s  ·  %s at work" % [
		Parts.thousands(republic.population()), Parts.thousands(republic.employed()),
	])
	_write("standing", "%d buildings  ·  %d vehicles" % [
		republic.building_count(), republic.vehicle_count(),
	])
	var money: PackedFloat32Array = republic.purse()
	_write("treasury", "%s ₽  ·  %s $" % [
		Parts.thousands(money[0] if money.size() > 0 else 0.0),
		Parts.thousands(money[1] if money.size() > 1 else 0.0),
	])

	_refresh_contentment(republic)
	_refresh_crews(republic)
	_refresh_movement(republic)
	_refresh_networks(republic)

	_refresh_stock(republic)
	_refresh_people(republic)

	var name: String = Overlays.NAMES[overlay_mode]
	_hint_overlay = "OVERLAY %s" % (name.to_upper() if name != "" else "NONE")
	# **How coarse the picture is, said on the picture.** The painted overlays
	# are sampled off the traversal lattice, not off the terrain: a 6 km map is a
	# 60x60 texture, so a red patch is at best one cell across and a player
	# reading it as "this hillside" rather than "this hundred metres" is reading
	# it wrong. The survey draws deposits at their real radius and is not sampled
	# at all, so it says nothing.
	if overlay_mode != Overlays.Mode.NONE and overlay_mode != Overlays.Mode.SURVEY:
		var cell: float = republic.lattice_cell_size()
		if cell > 0.0:
			_hint_overlay += "  ·  %.0f M CELLS" % cell
	_show_hint()


func _refresh_weather(republic: Republic) -> void:
	var rain: float = republic.precipitation_mm()
	_weather.text = "%+.1f °C%s" % [
		republic.temperature_c(), "     %.0f mm" % rain if rain > 0.05 else "",
	]

	# Five days ahead. Heating follows today's temperature and never the month, so
	# a cold snap is an event you can be caught out by -- and a forecast is what
	# makes being caught out a mistake rather than an ambush.
	var ahead: PackedFloat32Array = republic.forecast(6)
	var days := PackedStringArray()
	for d in range(1, ahead.size() / 2):
		days.append("%+.0f" % ahead[d * 2])

	var snow: PackedFloat32Array = republic.snow()
	if snow.size() >= 2 and snow[0] > 0.01:
		_write("snow", "%.0f%% lying  ·  %.0f%% unswept" % [
			snow[0] * 100.0, snow[1] * 100.0,
		], P.ALARM if snow[1] > 0.4 else P.PAPER)
	else:
		_write("snow", "none", P.PAPER_FAINT)
	_write("ahead", " ".join(days))


## How the republic is treating its people.
##
## The breakdown rather than the score: "your people are at 61%" is not something
## a player can act on and "fed, warm, no doctor, no work" is. So the panel names
## the weakest component beside the overall.
func _refresh_contentment(republic: Republic) -> void:
	var content: PackedFloat32Array = republic.contentment()
	var health: PackedFloat32Array = republic.wellbeing()
	# The components, then the overall, then the comfort lift -- so the two
	# trailing figures are counted rather than indexed by a number typed here.
	if content.size() != _content_names.size() + 2 or health.size() < 2:
		_write("content", "—")
		_write("wellbeing", "—")
		return
	var overall: float = content[content.size() - 2]
	var lift: float = content[content.size() - 1]
	var text := "%d%%  ·  weakest %s" % [int(round(overall * 100.0)), _weakest(content)]
	# Comforts add rather than deduct, so they are shown as what they are: a bonus
	# the republic earned. Only when there is one, because a permanent "+0%" would
	# read as a component that is failing.
	if lift > 0.005:
		text += "  ·  +%d%%" % int(round(lift * 100.0))
	# Below the threshold that attracts anybody is worth colouring: it is the line
	# between a republic that grows and one that only shrinks.
	_write("content", text, P.ALARM if overall < 0.6 else P.PAPER)
	_write("wellbeing", "%d%%  /  %d%%" % [
		int(round(health[0] * 100.0)), int(round(health[1] * 100.0)),
	])


## The component costing the republic most, by the simulation's own weighting.
func _weakest(content: PackedFloat32Array) -> String:
	var parts := _content_names.size()
	if parts == 0 or content.size() < parts:
		return "—"
	# The weights are the simulation's; the shell reads the parts and names the
	# lowest, which is a presentation choice rather than balance.
	var worst := 0
	for i in range(1, parts):
		if content[i] < content[worst]:
			worst = i
	if content[worst] > 0.98:
		return "nothing"
	return "%s %d%%" % [_content_names[worst], int(round(content[worst] * 100.0))]


## Where the republic's builders are.
##
## Work happens where the crews are standing, so a site with nobody on it and a
## site with a gang and no bricks look identical without this. The waiting count
## is the one that costs: a gang beside a finished building is people no office
## can post anywhere until a bus goes and fetches them.
func _refresh_crews(republic: Republic) -> void:
	var parties: PackedFloat32Array = republic.crew_parties()
	var stride := 5
	var riding := 0
	var working := 0
	var waiting := 0
	var heads := 0
	for i in range(0, parties.size() / stride):
		heads += int(parties[i * stride + 2])
		match int(parties[i * stride + 3]):
			0: riding += 1
			1: working += 1
			_: waiting += 1

	if heads == 0:
		_write("builders", "none out", P.PAPER_FAINT)
	else:
		_write(
			"builders",
			"%d out  ·  %d on site  ·  %d riding  ·  %d awaiting a bus"
				% [heads, working, riding, waiting],
			# A gang with nowhere to be is the state worth colouring: idle people,
			# and a bus journey the republic still owes them.
			P.ALARM if waiting > 0 else P.PAPER
		)

	# Foreign labour is the one standing cost in hard currency the republic
	# carries, and nothing else on this panel would show it: domestic builders cost
	# no money at all, so a wage bill has to be visible beside the treasury it
	# comes out of.
	var hired: PackedInt32Array = republic.hired_builders()
	if hired.size() >= 2 and (hired[0] + hired[1]) > 0:
		var terms: PackedFloat32Array = republic.hiring_terms()
		var daily := float(hired[0] + hired[1]) * (terms[1] if terms.size() > 1 else 0.0)
		_write("hired", "%d  ·  %s a day" % [hired[0] + hired[1], Parts.thousands(daily)])
	else:
		_write("hired", "none", P.PAPER_FAINT)


func _refresh_movement(republic: Republic) -> void:
	var totals: PackedInt32Array = republic.migration_totals()
	if totals.size() < 5:
		return
	_write("settlers", "%d settled  ·  %d left  ·  %d turned back" % [
		totals[2], totals[3], totals[4],
	])
	var waiting := totals[0]
	if waiting > 0:
		# People standing at a post are people the republic has been offered and has
		# not collected. That is a coach journey it owes them, and a season from now
		# they go home.
		_write("waiting", "%d in %d group%s" % [
			waiting, totals[1], "" if totals[1] == 1 else "s",
		], P.ALARM)
	else:
		_write("waiting", "nobody", P.PAPER_FAINT)

	# Tourism is the same fact from the other side: people at the border who need
	# collecting, out of the same pool of coaches. It says "none" until the
	# republic has beds, rather than showing a row of zeroes about a mechanic the
	# player has not opened.
	var visitors: PackedFloat32Array = republic.tourism()
	if visitors.size() >= 7 and (visitors[4] > 0.0 or visitors[0] > 0.0):
		_write("visitors", "%d staying  ·  %d beds  ·  %s ₽ / %s $" % [
			int(visitors[0]), int(visitors[4]),
			Parts.thousands(visitors[5]), Parts.thousands(visitors[6]),
		])
	else:
		_write("visitors", "none", P.PAPER_FAINT)


## The ways through the republic and the two networks over it.
##
## Water is on the ways line beside the built ones for a reason that is easy to
## miss: it is the one network nobody builds, so a republic with forty kilometres
## of navigable river and no port has an enormous asset it has not noticed.
##
## Both lines are sized off their rosters rather than off names typed here.
## `Medium::ALL` went from one to four and `Utility::ALL` from two to four, and a
## line with a hardcoded length silently stops mentioning half of what exists.
func _refresh_networks(republic: Republic) -> void:
	var lengths: PackedFloat32Array = republic.way_lengths()
	var fleet: PackedFloat32Array = republic.fleet_by_medium()
	var ways := _way_names.size()
	var parts := PackedStringArray()
	if ways > 0 and lengths.size() == ways and fleet.size() == ways * 2:
		for i in ways:
			# Air has no length worth printing -- it is a straight line between
			# aerodromes -- so it earns its place only once something flies.
			if lengths[i] > 0.05 or fleet[i] > 0.0:
				parts.append("%s %.1f km (%d/%d)" % [
					_way_names[i].to_lower(), lengths[i],
					int(fleet[ways + i]), int(fleet[i]),
				])
	_write("ways", "  ·  ".join(parts) if parts.size() > 0 else "none",
		P.PAPER if parts.size() > 0 else P.PAPER_FAINT)

	# **Two numbers, because they are two different problems.** Built lamps are
	# what the republic paid for; burning lamps are what is actually alight, and
	# the gap between them is a grid that has run short -- a thing to fix rather
	# than a thing to build, which is the opposite instruction from "lay more
	# lit road". A single figure could not tell the player which one they are in.
	var lit: PackedFloat32Array = republic.lit_road_km()
	if lit.size() < 2 or lit[0] < 0.05:
		_write("lit", "none", P.PAPER_FAINT)
	elif lit[1] < lit[0] - 0.05:
		_write("lit", "%.1f km laid  ·  %.1f km alight" % [lit[0], lit[1]], P.ALARM)
	else:
		_write("lit", "%.1f km, all alight" % lit[0])

	var grid: PackedFloat32Array = republic.utility_totals()
	var kinds := _utility_names.size()
	var strung := PackedStringArray()
	var dark := 0
	var cold := 0
	if kinds > 0 and grid.size() == kinds * 2 + 2:
		for i in kinds:
			if grid[i] > 0.05:
				strung.append("%s %.1f km (%d)" % [
					_utility_names[i].to_lower(), grid[i], int(grid[kinds + i]),
				])
		dark = int(grid[kinds * 2])
		cold = int(grid[kinds * 2 + 1])
	_write("grid", "  ·  ".join(strung) if strung.size() > 0 else "nothing strung",
		P.PAPER if strung.size() > 0 else P.PAPER_FAINT)

	# A republic can be short of generation or short of wire, and those are
	# different problems with the same symptom -- which is why this sits under the
	# kilometres rather than beside a plant.
	if dark > 0 or cold > 0:
		_write("unserved", "%d dark  ·  %d cold" % [dark, cold], P.ALARM)
	else:
		_write("unserved", "nothing", P.PAPER_FAINT)

	# Where sites buy what the republic cannot make. Off until a post is named,
	# because auto-import spends hard currency and a default that spent it would
	# be spending it before the player chose a bloc.
	var post: int = republic.import_post()
	if post == 0:
		_write("imports", "none", P.PAPER_FAINT)
	else:
		_write("imports", "through post %d" % post)


func _refresh_stock(republic: Republic) -> void:
	var held: PackedFloat32Array = republic.stockpiles()
	for row in _stock_rows:
		var name_label := _stock_labels[row * 2]
		var value := _stock_labels[row * 2 + 1]
		if row >= held.size() or row >= _resource_names.size():
			name_label.text = ""
			value.text = ""
			continue
		# The form goes on the name, not in a column of its own: it is what decides
		# which store will take the goods, and a player reading "Fuel (Liquid)"
		# already knows why the grain silo refused it.
		if row < _resource_forms.size():
			name_label.text = "%s  (%s)" % [_resource_names[row], _resource_forms[row]]
		else:
			name_label.text = _resource_names[row]
		value.text = "%.1f t" % held[row]
		# Nothing at all is worth saying quietly rather than not at all: an empty
		# bin is the most important number on this table.
		var empty := held[row] < 0.05
		name_label.add_theme_color_override(
			"font_color", P.PAPER_FAINT if empty else P.PAPER_DIM
		)
		value.add_theme_color_override("font_color", P.ALARM if empty else P.PAPER)


## Who the republic is made of, and how well it is serving them.
##
## Everything the demographic model knows, on one table. Without it life stages,
## education and the contentment breakdown are all facts the simulation has and
## the player cannot see -- and a republic whose pupils outnumber its workers is
## about to be short of hands in a way no population count shows.
func _refresh_people(republic: Republic) -> void:
	var who: PackedInt32Array = republic.demographics()
	var content: PackedFloat32Array = republic.contentment()

	var rows: Array = []
	if who.size() >= 8:
		rows.append(["BY AGE", "", true])
		for i in STAGE_NAMES.size():
			rows.append([STAGE_NAMES[i], str(who[i]), false])
		rows.append(["SCHOOLING", "", true])
		for i in LEARNING_NAMES.size():
			rows.append([LEARNING_NAMES[i], str(who[STAGE_NAMES.size() + i]), false])
	var parts := _content_names.size()
	if parts > 0 and content.size() == parts + 2:
		rows.append(["CONTENTMENT", "%d%%" % int(round(content[parts] * 100.0)), true])
		for i in parts:
			rows.append([_content_names[i], "%d%%" % int(round(content[i] * 100.0)), false])
		# Last, and signed, because it is not one of the components above: those are
		# ways to fail and this is a bonus. A player reading down the column must not
		# be invited to go and fix it.
		rows.append(["Comforts", "+%d%%" % int(round(content[parts + 1] * 100.0)), false])

	for row in _people_rows:
		var name_label := _people_labels[row * 2]
		var value := _people_labels[row * 2 + 1]
		if row >= rows.size():
			name_label.text = ""
			value.text = ""
			continue
		name_label.text = rows[row][0]
		value.text = rows[row][1]
		var heading: bool = rows[row][2]
		name_label.add_theme_color_override(
			"font_color", P.OCHRE if heading else P.PAPER_DIM
		)
		var tone := P.PAPER
		if not heading and value.text.ends_with("%") and not value.text.begins_with("+"):
			if int(value.text.trim_suffix("%")) < 50:
				tone = P.ALARM
		value.add_theme_color_override("font_color", tone)


## Write one row of the left-hand panel by name.
##
## Named rather than indexed, because a panel of sixteen rows addressed by number
## is a panel where inserting one shifts fifteen writes by one and nothing says
## so.
func _write(key: String, text: String, tone: Color = P.PAPER) -> void:
	var label: Label = _state.get(key)
	if label == null:
		return
	label.text = text
	label.add_theme_color_override("font_color", tone)


# ---- the hint line -----------------------------------------------------------


func set_hint(text: String) -> void:
	_hint_keys = text
	_show_hint()


## What is in hand while placing, why the ground under the cursor refuses it, and
## what that ground is like.
##
## The refusal is a sentence rather than a code, because `Command` was built to
## return one. An empty `kind` clears the line back to the keys.
##
## `note` is the ground: how the going is here, how far the frontier is, what a
## hotel sited here would be worth. It is **separate from the refusal** rather
## than appended to it, because the refusal is the one thing this line ever says
## in the alarm colour — and painting "firm going, 900 m from the frontier" red
## would be telling the player something is wrong with ground that is fine.
func set_placing(kind: String, why: String, note: String = "") -> void:
	if kind == "":
		_hint_placing = ""
	elif why == "":
		_hint_placing = "PLACING %s  ·  left click to build, right click or esc to stop%s" % [
			kind.to_upper(), "     %s" % note if note != "" else "",
		]
	else:
		_hint_placing = "PLACING %s  ·  %s" % [kind.to_upper(), why]
	_refusing = why != "" and kind != ""
	_show_hint()


## Whether the ground under the cursor is refusing what is in hand, which is the
## one thing this line ever says in the alarm colour.
var _refusing := false


## Put the most urgent of the three on the line.
##
## **Placing wins, and it has to.** `refresh` runs after the ghost update every
## frame, so a hint line that simply wrote whatever it was last told would show
## the key list over a live refusal -- which is the one moment the player is
## actually reading it.
func _show_hint() -> void:
	if _hint_placing != "":
		_hint.text = _hint_placing
		_hint.add_theme_color_override(
			"font_color", P.ALARM if _refusing else P.PAPER_DIM
		)
		return
	_hint.text = (
		"%s     %s" % [_hint_overlay, _hint_keys] if _hint_overlay != "" else _hint_keys
	)
	_hint.add_theme_color_override("font_color", P.PAPER_FAINT)
