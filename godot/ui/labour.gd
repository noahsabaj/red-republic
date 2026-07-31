extends CanvasLayer

## The roster: how much of the day the republic is awake for, and who is awake.
##
## **Night is not a lighting effect in this game, it is a labour problem.** A
## building runs the hours somebody is standing in it and no others, so this
## screen is where the day gets decided. Three levers, deliberately different
## animals:
##
## **Crews** — how many shifts a workplace runs. Three is round the clock: three
## times the goods, and three times the people, who have to be housed, fed and
## carried to work in the dark. A straight trade.
##
## **Hours** — how long one shift is. This takes no extra people at all, which is
## exactly why it costs the ones you have: health and loyalty fall in proportion
## to the hours past eight, and loyalty is what makes people leave.
##
## **Standing** — who gets people first when there are not enough for everyone.
## The other two levers are about a works the republic can already man; this is
## the one that decides which works that is, and it is the sharpest control on the
## screen. A republic short of hands fills First before Ordinary and Ordinary
## before Last, so ranking the power plant above the offices is the difference
## between a town with lights and a town without.
##
## # Three levels, because that is how the question gets asked
##
## A national standard, a rule for a category, an exception for one building.
## *"Doctors work twelve, but at this hospital fourteen."* The narrowest rule
## wins, and every row says which level its number came from — because "12" and
## "12, because you set it here" send a player to different controls.
##
## # Built once, shown many times
##
## Same rule as the build menu: a `Label` costs 165x more to build than to update,
## so rows are constructed on the first open and afterwards only refreshed. A
## republic with two hundred workplaces would otherwise hitch every single time.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal closed

const PLACE_COLUMNS := [
	["workplace", 2.6],
	["manning", 1.3, HORIZONTAL_ALIGNMENT_RIGHT],
	["the day", 2.0, HORIZONTAL_ALIGNMENT_RIGHT],
	["crews", 1.0, HORIZONTAL_ALIGNMENT_RIGHT],
	["hours", 1.3, HORIZONTAL_ALIGNMENT_RIGHT],
	["standing", 1.2, HORIZONTAL_ALIGNMENT_RIGHT],
]
const TRADE_COLUMNS := [
	["trade", 2.4],
	["rule", 1.2, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 1.4, HORIZONTAL_ALIGNMENT_RIGHT],
]

## Where a building's working day came from, in the order `views::workplaces`
## reports it.
const RULE_NAMES := ["standard", "by trade", "this one"]

var _republic: Republic = null
var _kinds: VBoxContainer = null
var _rows: VBoxContainer = null
var _national: Label = null
var _summary: Label = null
var _refusal: Label = null
var _built := false

## `[min_hours, max_hours, max_shifts]`, read from the simulation so a control can
## never offer a value the command will refuse.
var _limits := PackedFloat32Array()

## One row's labels, by building id, so a refresh writes numbers rather than
## rebuilding nodes.
var _row_labels := {}
var _kind_labels := {}


func _ready() -> void:
	layer = 13
	visible = false


func open(republic: Republic) -> void:
	_republic = republic
	_limits = republic.shift_limits()
	if not _built:
		_build()
		_built = true
	refresh()
	visible = true


func close() -> void:
	visible = false
	closed.emit()


func _min_hours() -> float:
	return _limits[0] if _limits.size() > 0 else 4.0


func _max_hours() -> float:
	return _limits[1] if _limits.size() > 1 else 16.0


func _max_shifts() -> int:
	return int(_limits[2]) if _limits.size() > 2 else 3


func _build() -> void:
	var sheet: Dictionary = Sheet.build(
		self,
		"Labour",
		"Ministry of Labour",
		"A building runs the hours somebody is standing in it. More crews is more goods "
		+ "and more people; a longer shift is more goods out of the same people, and it "
		+ "costs them their health and their loyalty."
	)
	var body: VBoxContainer = sheet["body"]
	_refusal = sheet["notice"]

	_summary = Parts.say("", "Figure")
	body.add_child(_summary)

	# --- The national standard, in a well of its own --------------------------
	var well := PanelContainer.new()
	well.theme_type_variation = "Well"
	body.add_child(well)

	var national_line := HBoxContainer.new()
	national_line.add_theme_constant_override("separation", P.GAP_WIDE)
	national_line.alignment = BoxContainer.ALIGNMENT_CENTER
	well.add_child(national_line)

	national_line.add_child(Parts.field("the working day"))
	_national = Parts.figure("", "FigureBig")
	_national.custom_minimum_size = Vector2(110, 0)
	national_line.add_child(_national)
	national_line.add_child(Parts.stepper(func(delta: float): _nudge_national(delta)))
	national_line.add_child(Parts.fill())
	national_line.add_child(Parts.say(
		"Every workplace with no rule of its own.", "Faint"
	))

	# **Two columns, and that came out of looking at a rendered frame.** Stacked,
	# sixteen trade rows filled the screen on their own and the workplace table —
	# the half a player acts on — sat entirely below the fold, while the right
	# third of every row was empty.
	var columns := Parts.columns(body)

	var left := Parts.section(
		columns,
		"Every workplace",
		"What each place runs today. Crews cost people; hours cost the people you have.",
		2.4
	)
	Parts.head(left, PLACE_COLUMNS)
	_rows = Parts.scroller(left)

	var right := Parts.section(
		columns,
		"By trade",
		"A rule about a kind covers every one of them, including the ones not built yet.",
		1.2
	)
	Parts.head(right, TRADE_COLUMNS)
	_kinds = Parts.scroller(right)

	Sheet.close_button(sheet["footer"], "BACK", close)


func _nudge_national(delta: float) -> void:
	var wanted := clampf(_republic.national_shift_hours() + delta, _min_hours(), _max_hours())
	_say(_republic.set_national_shift_hours(wanted))
	refresh()


## The rows exist for every kind the republic actually employs anybody in.
## Listing all hundred-odd would be a wall of "standard" — and a rule about a kind
## nothing has been built of is a rule with nothing to say yet.
func _rebuild_kind_rows(kinds: Array) -> void:
	for child in _kinds.get_children():
		child.queue_free()
	_kind_labels.clear()

	for i in kinds.size():
		var kind: int = kinds[i]
		var line := Parts.row(_kinds, i % 2 == 1)
		line.add_child(Parts.cell(
			Parts.say(String(_republic.building_kind_name(kind)), "Small"), TRADE_COLUMNS[0][1]
		))
		var value := Parts.figure("")
		line.add_child(Parts.cell(value, TRADE_COLUMNS[1][1], HORIZONTAL_ALIGNMENT_RIGHT))
		_kind_labels[kind] = value

		var controls := HBoxContainer.new()
		controls.alignment = BoxContainer.ALIGNMENT_END
		controls.add_theme_constant_override("separation", P.GAP_TIGHT)
		controls.add_child(Parts.stepper(func(delta: float): _nudge_kind(kind, delta)))
		# A reset rather than a sentence: the column is narrow and "use the
		# standard" spelled out crowded the number it was about.
		var clear := Parts.button("↺", "Step")
		clear.custom_minimum_size = Vector2(P.STEP_BUTTON, P.STEP_BUTTON)
		clear.tooltip_text = "Drop this rule and use the national standard"
		clear.pressed.connect(func(): _clear_kind(kind))
		controls.add_child(clear)
		line.add_child(Parts.cell(controls, TRADE_COLUMNS[2][1]))


func _kind_rule(kind: int) -> float:
	var rules: PackedFloat32Array = _republic.kind_shift_rules()
	var i := 0
	while i + 1 < rules.size():
		if int(rules[i]) == kind:
			return rules[i + 1]
		i += 2
	return -1.0


func _nudge_kind(kind: int, delta: float) -> void:
	# Starting from the national standard when there is no rule yet is what makes
	# the first press mean "nine" rather than "four": a new rule should begin where
	# the republic already is.
	var current := _kind_rule(kind)
	if current < 0.0:
		current = _republic.national_shift_hours()
	_say(_republic.set_kind_shift_hours(kind, clampf(current + delta, _min_hours(), _max_hours())))
	refresh()


func _clear_kind(kind: int) -> void:
	_say(_republic.set_kind_shift_hours(kind, -1.0))
	refresh()


func _rebuild_rows(ids: Array) -> void:
	for child in _rows.get_children():
		child.queue_free()
	_row_labels.clear()

	for i in ids.size():
		var id: int = ids[i]
		# **One line per workplace now, not two.** The old row stacked a name over
		# a bank of seven controls, which meant nothing lined up with anything on
		# the row above it — the screen read as a stack of cards rather than a
		# table. Columns are what make forty workplaces comparable at a glance,
		# which is the entire question this screen exists to answer.
		var line := Parts.row(_rows, i % 2 == 1)

		var name_label := Parts.say("", "Small")
		line.add_child(Parts.cell(name_label, PLACE_COLUMNS[0][1]))

		var staff := Parts.figure("")
		line.add_child(Parts.cell(staff, PLACE_COLUMNS[1][1], HORIZONTAL_ALIGNMENT_RIGHT))

		var roster := Parts.figure("")
		line.add_child(Parts.cell(roster, PLACE_COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT))

		line.add_child(Parts.cell(
			Parts.stepper(func(delta: float): _nudge_shifts(id, int(delta))),
			PLACE_COLUMNS[3][1]
		))

		var hours := HBoxContainer.new()
		hours.alignment = BoxContainer.ALIGNMENT_END
		hours.add_theme_constant_override("separation", P.GAP_TIGHT)
		hours.add_child(Parts.stepper(func(delta: float): _nudge_building(id, delta)))
		var clear := Parts.button("↺", "Step")
		clear.custom_minimum_size = Vector2(P.STEP_BUTTON, P.STEP_BUTTON)
		clear.tooltip_text = "Drop this building's exception"
		clear.pressed.connect(func(): _clear_building(id))
		hours.add_child(clear)
		line.add_child(Parts.cell(hours, PLACE_COLUMNS[4][1]))

		# **The standing sits last, beside the manning it explains.** A works
		# reading "9 of 16 posts" is only half an answer; without the standing a
		# player cannot tell a republic that has run out of hands from one that has
		# ranked this place below everything else.
		var standing := Parts.button("", "Quiet")
		standing.pressed.connect(func(): _cycle_standing(id))
		var holder := HBoxContainer.new()
		holder.alignment = BoxContainer.ALIGNMENT_END
		holder.add_child(standing)
		line.add_child(Parts.cell(holder, PLACE_COLUMNS[5][1]))

		_row_labels[id] = [name_label, staff, roster, standing]


## Step one workplace up through the labour plan, wrapping at the top.
##
## A cycling button rather than three radio buttons or a dropdown: there are
## exactly three standings, stepping through costs at most two clicks, and a
## dropdown per row on a screen that can hold a hundred workplaces is a hundred
## popups nobody opens.
func _cycle_standing(id: int) -> void:
	var row: PackedFloat32Array = _workplace(id)
	if row.size() < 8:
		return
	var names: PackedStringArray = _republic.priority_names()
	var next := (int(row[7]) + 1) % maxi(names.size(), 1)
	_say(_republic.set_priority(id, next))
	refresh()


func _nudge_shifts(id: int, delta: int) -> void:
	var row: PackedFloat32Array = _workplace(id)
	if row.size() < 5:
		return
	_say(_republic.set_shifts(id, clampi(int(row[4]) + delta, 0, _max_shifts())))
	refresh()


func _nudge_building(id: int, delta: float) -> void:
	var row: PackedFloat32Array = _workplace(id)
	if row.size() < 6:
		return
	_say(_republic.set_building_shift_hours(
		id, clampf(row[5] + delta, _min_hours(), _max_hours())
	))
	refresh()


func _clear_building(id: int) -> void:
	_say(_republic.set_building_shift_hours(id, -1.0))
	refresh()


## One workplace's line out of the packed sweep, or an empty array.
func _workplace(id: int) -> PackedFloat32Array:
	var packed: PackedFloat32Array = _republic.workplaces()
	var stride := 8
	var i := 0
	while i + stride <= packed.size():
		if int(packed[i]) == id:
			return packed.slice(i, i + stride)
		i += stride
	return PackedFloat32Array()


func refresh() -> void:
	if _republic == null or _rows == null:
		return

	_national.text = "%s h" % Parts.clean(_republic.national_shift_hours())

	var packed: PackedFloat32Array = _republic.workplaces()
	var stride := 8
	var ids := []
	var kinds := []
	var jobs := 0
	var staffed := 0
	var hours_open := 0.0
	var i := 0
	while i + stride <= packed.size():
		ids.append(int(packed[i]))
		var kind := int(packed[i + 1])
		if not kinds.has(kind):
			kinds.append(kind)
		staffed += int(packed[i + 2])
		jobs += int(packed[i + 3])
		hours_open += minf(packed[i + 4] * packed[i + 5], 24.0)
		i += stride

	# Rebuild only when the set of workplaces changed. A republic gains a building
	# rarely and refreshes this screen every time a button is pressed.
	if ids != _row_labels.keys():
		_rebuild_rows(ids)
	if kinds != _kind_labels.keys():
		_rebuild_kind_rows(kinds)

	var places := ids.size()
	_summary.text = "%d workplaces      %d of %d posts filled      %.1f hours open on average" % [
		places, staffed, jobs, (hours_open / places) if places > 0 else 0.0
	]

	for kind in _kind_labels:
		var rule := _kind_rule(kind)
		var label: Label = _kind_labels[kind]
		label.text = "standard" if rule < 0.0 else "%s h" % Parts.clean(rule)
		label.add_theme_color_override(
			"font_color", P.PAPER_FAINT if rule < 0.0 else P.PAPER
		)

	var standings: PackedStringArray = _republic.priority_names()
	i = 0
	while i + stride <= packed.size():
		var id := int(packed[i])
		if _row_labels.has(id):
			var parts: Array = _row_labels[id]
			var shifts := int(packed[i + 4])
			var hours: float = packed[i + 5]
			var rule := int(packed[i + 6])
			var manned := int(packed[i + 2])
			var posts := int(packed[i + 3])
			parts[0].text = String(_republic.building_kind_name(int(packed[i + 1])))
			parts[1].text = "%d / %d" % [manned, posts]
			# **Coloured by whether this place got the people it asked for, not by
			# which standing it holds.** A full "Last" works needs no attention; a
			# half-empty "First" one is the republic saying it has run out of hands,
			# which is a different problem from a bad plan and wants a different
			# answer.
			parts[1].add_theme_color_override(
				"font_color", P.PAPER if manned >= posts else P.ALARM
			)
			var rank := clampi(int(packed[i + 7]), 0, maxi(standings.size() - 1, 0))
			parts[3].text = (
				String(standings[rank]).to_upper() if standings.size() > 0 else "—"
			)
			if shifts == 0:
				parts[2].text = "closed"
				parts[2].add_theme_color_override("font_color", P.ALARM)
			else:
				parts[2].text = "%d x %s = %s h (%s)" % [
					shifts,
					Parts.clean(hours),
					Parts.clean(minf(shifts * hours, 24.0)),
					RULE_NAMES[clampi(rule, 0, RULE_NAMES.size() - 1)],
				]
				parts[2].add_theme_color_override(
					"font_color", P.ALARM if hours > 8.0 else P.PAPER_DIM
				)
		i += stride


## Print a refusal, or clear the last one. Every setter here hands back the
## simulation's own sentence rather than the panel inventing wording of its own.
func _say(why: String) -> void:
	_refusal.text = why
