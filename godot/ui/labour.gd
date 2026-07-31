extends CanvasLayer

## The roster: how much of the day the republic is awake for, and who is awake.
##
## **Night is not a lighting effect in this game, it is a labour problem.** A
## building runs the hours somebody is standing in it and no others, so this
## screen is where the day gets decided. Two levers, and they are deliberately
## different animals:
##
## **Shifts** — how many crews a workplace runs. Three is round the clock: three
## times the goods, and three times the people, who have to be housed, fed and
## carried to work in the dark. A straight trade.
##
## **Hours** — how long one shift is. This one takes no extra people at all,
## which is exactly why it costs the ones you have: health and loyalty fall in
## proportion to the hours past eight, and loyalty is what makes people leave.
##
## **Standing** — who gets people first when there are not enough for everyone.
## The other two levers are about a works the republic can already man; this is
## the one that decides which works that is, and it is the sharpest control on
## the screen. A republic short of hands fills First before Ordinary and
## Ordinary before Last, so ranking the power plant above the offices is the
## difference between a town with lights and a town without. Every kind opens at
## a sensible standing and this is how the player disagrees.
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
## Same rule as the build menu: a `Label` costs 165x more to build than to
## update, so the rows are constructed on the first open and afterwards only
## refreshed. A republic with two hundred workplaces would otherwise hitch every
## single time this screen opened.

const Style := preload("res://ui/theme.gd")

signal closed

var _republic: Republic = null
var _kinds: VBoxContainer = null
var _rows: VBoxContainer = null
var _national: Label = null
var _summary: Label = null
var _refusal: Label = null
var _built := false

## `[min_hours, max_hours, max_shifts]`, read from the simulation so a control
## can never offer a value the command will refuse.
var _limits := PackedFloat32Array()

## One row's controls, by building id, so a refresh writes numbers rather than
## rebuilding nodes.
var _row_labels := {}
var _kind_labels := {}
var _kind_names := PackedStringArray()

## Where a building's working day came from, in the order `views::workplaces`
## reports it.
const RULE_NAMES := ["standard", "by kind", "this one"]


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
	add_child(Style.backdrop(1.0))

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 72)
	margin.add_theme_constant_override("margin_right", 72)
	margin.add_theme_constant_override("margin_top", 56)
	margin.add_theme_constant_override("margin_bottom", 48)
	add_child(margin)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 10)
	margin.add_child(column)

	column.add_child(Style.heading("LABOUR", Style.SIZE_TITLE))
	column.add_child(Style.small(
		"A building runs the hours somebody is standing in it. More crews is more "
		+ "goods and more people; a longer shift is more goods out of the same people, "
		+ "and it costs them their health and their loyalty.",
		Style.INK_DIM
	))

	_summary = Style.body("", Style.INK)
	column.add_child(_summary)
	_refusal = Style.small("", Style.ALARM)
	column.add_child(_refusal)
	column.add_child(Style.divider())

	# --- The national standard ------------------------------------------------
	var national_line := HBoxContainer.new()
	national_line.add_theme_constant_override("separation", 12)
	var national_name := Style.body("The working day", Style.INK)
	national_name.custom_minimum_size = Vector2(240, 0)
	national_line.add_child(national_name)
	_national = Style.body("", Style.INK)
	_national.custom_minimum_size = Vector2(120, 0)
	national_line.add_child(_national)
	national_line.add_child(_hours_stepper(func(delta: float): _nudge_national(delta)))
	var national_spacer := Control.new()
	national_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	national_line.add_child(national_spacer)
	national_line.add_child(Style.small(
		"Every workplace with no rule of its own.", Style.INK_FAINT
	))
	var national_card := Style.card(Style.PAPER_RAISED, Style.RULE, 10)
	Style.card_body(national_card).add_child(national_line)
	column.add_child(national_card)

	# **Two columns, and that came out of looking at a rendered frame.** Stacked,
	# sixteen trade rows filled the screen on their own and the workplace table —
	# the half a player actually acts on — sat entirely below the fold, while the
	# right third of every row was empty. Side by side, both lists are visible at
	# once and each scrolls on its own.
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.add_theme_constant_override("separation", 24)
	column.add_child(columns)

	columns.add_child(_section(
		"EVERY WORKPLACE",
		"What each place runs today. Shifts cost people; hours cost the people you have.",
		2.0,
		func(rows: VBoxContainer): _rows = rows
	))
	columns.add_child(_section(
		"BY TRADE",
		"A rule about a kind covers every one of them, including the ones not built yet.",
		1.0,
		func(rows: VBoxContainer): _kinds = rows
	))

	column.add_child(Style.divider())
	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", 8)
	var back := Style.button("Back")
	back.pressed.connect(close)
	footer.add_child(back)
	column.add_child(footer)


## A titled, scrolling column. `keep` is handed the container the rows go in,
## because the two columns hold different lists and each wants its own field.
func _section(title: String, blurb: String, weight: float, keep: Callable) -> Control:
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.size_flags_stretch_ratio = weight
	box.add_theme_constant_override("separation", 4)
	box.add_child(Style.heading(title, Style.SIZE_HEAD))
	box.add_child(Style.small(blurb, Style.INK_FAINT))

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	box.add_child(scroll)

	var rows := VBoxContainer.new()
	rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rows.add_theme_constant_override("separation", 3)
	scroll.add_child(rows)
	keep.call(rows)
	return box


## A minus/plus pair for an hours figure. `nudge` takes the signed change.
func _hours_stepper(nudge: Callable) -> Control:
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	var down := Style.button("-")
	down.custom_minimum_size = Vector2(30, 0)
	down.pressed.connect(func(): nudge.call(-1.0))
	box.add_child(down)
	var up := Style.button("+")
	up.custom_minimum_size = Vector2(30, 0)
	up.pressed.connect(func(): nudge.call(1.0))
	box.add_child(up)
	return box


func _nudge_national(delta: float) -> void:
	var wanted := clampf(_republic.national_shift_hours() + delta, _min_hours(), _max_hours())
	_say(_republic.set_national_shift_hours(wanted))
	refresh()


## The rows exist for every kind the republic actually employs anybody in.
## Listing all hundred-odd would be a wall of "standard" — and a rule about a
## kind nothing has been built of is a rule with nothing to say yet.
func _rebuild_kind_rows(kinds: Array) -> void:
	for child in _kinds.get_children():
		child.queue_free()
	_kind_labels.clear()

	for kind in kinds:
		var line := HBoxContainer.new()
		line.add_theme_constant_override("separation", 8)

		var name_label := Style.body(String(_republic.building_kind_name(kind)), Style.INK)
		name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		line.add_child(name_label)

		var value := Style.body("", Style.INK_DIM)
		value.custom_minimum_size = Vector2(86, 0)
		value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		line.add_child(value)
		_kind_labels[kind] = value

		line.add_child(_hours_stepper(func(delta: float): _nudge_kind(kind, delta)))

		# A reset rather than a sentence: the column is narrow and "Use standard"
		# spelled out crowded the number it was about.
		var clear := Style.button("↺")
		clear.custom_minimum_size = Vector2(34, 0)
		clear.tooltip_text = "Drop this rule and use the national standard"
		clear.pressed.connect(func(): _clear_kind(kind))
		line.add_child(clear)

		var holder := Style.card(Style.PAPER_RAISED, Style.RULE, 7)
		Style.card_body(holder).add_child(line)
		_kinds.add_child(holder)


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
	# the first press mean "twelve" rather than "four": a new rule should begin
	# where the republic already is.
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

	for id in ids:
		# **Two lines per card, not one.** Seven controls on one row ran off the
		# side of a column and the fixed widths that stopped it doing so left the
		# name clipped. The name and the manning belong together, and the roster
		# belongs with the controls that change it.
		var stack := VBoxContainer.new()
		stack.add_theme_constant_override("separation", 2)

		var top := HBoxContainer.new()
		top.add_theme_constant_override("separation", 8)
		var name_label := Style.body("", Style.INK)
		name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		top.add_child(name_label)
		# **Name, then manning, then standing**, in that order because that is the
		# order the question gets asked: what is this, how is it doing, and where
		# did I put it in the plan. The standing sat between the name and the
		# manning on the first writing, which left a button floating in the middle
		# of the row with the number it explains stranded on the far side of it.
		#
		# It is on the top line rather than down with the crew and hour controls
		# because it answers the number beside it: a works reading "9 of 16 posts"
		# is only half an answer, and without the standing a player cannot tell a
		# republic that has run out of hands from one that has ranked this place
		# below everything else.
		var staff := Style.small("", Style.INK_DIM)
		staff.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		top.add_child(staff)
		var standing := Style.button("")
		standing.custom_minimum_size = Vector2(96, 0)
		standing.pressed.connect(func(): _cycle_standing(id))
		top.add_child(standing)
		stack.add_child(top)

		var bottom := HBoxContainer.new()
		bottom.add_theme_constant_override("separation", 6)
		var roster := Style.small("", Style.INK)
		roster.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		bottom.add_child(roster)

		bottom.add_child(Style.small("crews", Style.INK_FAINT))
		var fewer := Style.button("-")
		fewer.custom_minimum_size = Vector2(30, 0)
		fewer.pressed.connect(func(): _nudge_shifts(id, -1))
		bottom.add_child(fewer)
		var more := Style.button("+")
		more.custom_minimum_size = Vector2(30, 0)
		more.pressed.connect(func(): _nudge_shifts(id, 1))
		bottom.add_child(more)

		bottom.add_child(Style.small("hours", Style.INK_FAINT))
		bottom.add_child(_hours_stepper(func(delta: float): _nudge_building(id, delta)))

		var clear := Style.button("↺")
		clear.custom_minimum_size = Vector2(30, 0)
		clear.tooltip_text = "Drop this building's exception"
		clear.pressed.connect(func(): _clear_building(id))
		bottom.add_child(clear)
		stack.add_child(bottom)

		var holder := Style.card(Style.PAPER_RAISED, Style.RULE, 7)
		Style.card_body(holder).add_child(stack)
		_rows.add_child(holder)
		_row_labels[id] = [name_label, staff, roster, standing]


## Step one workplace up through the labour plan, wrapping at the top.
##
## A cycling button rather than three radio buttons or a dropdown: there are
## exactly three standings, the list is short enough that stepping through it
## costs at most two clicks, and a dropdown per row on a screen that can hold a
## hundred workplaces is a hundred popups nobody opens.
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

	_national.text = "%s hours" % _hours_text(_republic.national_shift_hours())

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

	# Rebuild only when the set of workplaces changed. A republic gains a
	# building rarely and refreshes this screen every time a button is pressed.
	if ids != _row_labels.keys():
		_rebuild_rows(ids)
	if kinds != _kind_labels.keys():
		_rebuild_kind_rows(kinds)

	var places := ids.size()
	_summary.text = "%d workplaces  ·  %d of %d posts filled  ·  %.1f hours open on average" % [
		places, staffed, jobs, (hours_open / places) if places > 0 else 0.0
	]

	for kind in _kind_labels:
		var rule := _kind_rule(kind)
		var label: Label = _kind_labels[kind]
		if rule < 0.0:
			label.text = "standard"
			label.add_theme_color_override("font_color", Style.INK_FAINT)
		else:
			label.text = "%s hours" % _hours_text(rule)
			label.add_theme_color_override("font_color", Style.INK)

	i = 0
	while i + stride <= packed.size():
		var id := int(packed[i])
		if _row_labels.has(id):
			var parts: Array = _row_labels[id]
			var shifts := int(packed[i + 4])
			var hours: float = packed[i + 5]
			var rule := int(packed[i + 6])
			parts[0].text = String(_republic.building_kind_name(int(packed[i + 1])))
			parts[1].text = "%d of %d posts" % [int(packed[i + 2]), int(packed[i + 3])]
			# **Coloured by whether this place got the people it asked for, not
			# by which standing it holds.** A full "Last" works needs no
			# attention; a half-empty "First" one is the republic saying it has
			# run out of hands, which is a different problem from a bad plan and
			# wants a different answer.
			var standings: PackedStringArray = _republic.priority_names()
			var rank := clampi(int(packed[i + 7]), 0, maxi(standings.size() - 1, 0))
			parts[3].text = String(standings[rank]) if standings.size() > 0 else "-"
			parts[3].add_theme_color_override(
				"font_color",
				Style.INK if int(packed[i + 2]) >= int(packed[i + 3]) else Style.ALARM
			)
			if shifts == 0:
				parts[2].text = "closed"
				parts[2].add_theme_color_override("font_color", Style.ALARM)
			else:
				parts[2].text = "%d x %s h  =  %s h  (%s)" % [
					shifts,
					_hours_text(hours),
					_hours_text(minf(shifts * hours, 24.0)),
					RULE_NAMES[clampi(rule, 0, RULE_NAMES.size() - 1)],
				]
				parts[2].add_theme_color_override(
					"font_color", Style.ALARM if hours > 8.0 else Style.INK
				)
		i += stride


## Print a refusal, or clear the last one. Every setter here hands back the
## simulation's own sentence rather than the panel inventing wording of its own.
func _say(why: String) -> void:
	_refusal.text = why


func _hours_text(hours: float) -> String:
	return "%d" % int(round(hours)) if is_equal_approx(hours, round(hours)) else "%.1f" % hours
