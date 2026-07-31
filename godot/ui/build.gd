extends CanvasLayer

## The build menu: what a republic can put up, and what it costs to have it put up.
##
## **This is the screen the game could not be played without.** The shell has
## exposed `place()` since M5 and nothing ever called it — there was no way to
## build anything, which went unnoticed for as long as the founding handed the
## player a working town to watch. The map is empty now, so a republic with no
## build menu is a republic with nothing in it and nothing to do.
##
## # Two ways to raise a building, and the difference is the early game
##
## **Build it yourself** — a site your Construction Offices work with your crews
## and your materials. Costs no money at all and is what a republic should be
## doing for nearly everything.
##
## **Contract it** — a foreign firm raises it, needing no crew and no materials,
## and bills the treasury daily in that bloc's own currency. Several times the
## price, and the only thing that works on day one, because a blank map has no
## offices, no crews and nobody to staff them.
##
## The panel shows both prices side by side for exactly that reason: the opening
## is a question of what you can afford to have built for you, and the moment an
## office exists the answer changes.
##
## # Built once, shown many times
##
## A hundred-odd rows of four labels is about 400 `Label` nodes, and building one
## costs 165x what updating it does — measured. So the list is constructed on the
## first open and afterwards only ever shown and hidden. Rebuilding it per open
## would be a visible hitch every single time.

const Style := preload("res://ui/theme.gd")

## Emitted when the player picks something to place. `market` is -1 to build it
## with your own crews, or 0 East / 1 West to contract it out.
signal chose(kind: int, market: int)
## Emitted when the player picks a way to lay. `lamps` asks for street lighting,
## which only paved road may carry.
signal chose_way(grade: int, lamps: bool)
signal closed

var _republic: Republic = null
var _rows: VBoxContainer = null
var _purse: Label = null
var _built := false

## Whether the republic has anybody to work a site of its own, and what that
## means for the Build button. Refreshed on every open.
var _crews: Label = null


func _ready() -> void:
	layer = 12
	visible = false


func open(republic: Republic) -> void:
	_republic = republic
	if not _built:
		_build()
		_built = true
	_refresh_purse()
	_refresh_own()
	visible = true


func close() -> void:
	visible = false
	closed.emit()


func _build() -> void:
	# Opaque. At 0.86 the HUD read straight through a full-screen list and the
	# whole thing looked like two screens printed on top of each other; 0.97 was
	# the first attempt at that and a rendered frame showed it still ghosting,
	# because the paper is dark and the text over it is not.
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

	column.add_child(Style.heading("BUILD", Style.SIZE_TITLE))
	_purse = Style.small("", Style.INK_DIM)
	column.add_child(_purse)
	_crews = Style.small("", Style.INK_DIM)
	column.add_child(_crews)
	column.add_child(Style.small(
		"Build it yourself with your own crews and materials, which costs no money "
		+ "and takes as long as you have hands for — or pay a foreign firm, which "
		+ "needs neither and starts the same day.",
		Style.INK_DIM
	))
	column.add_child(Style.divider())

	# **Two columns: things that stand still, and things that go somewhere.**
	# A road was as unbuildable from this game as a factory was before this menu
	# existed, and on a map that starts empty a republic that cannot lay road
	# cannot connect anything it builds.
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.clip_contents = true
	columns.add_theme_constant_override("separation", 24)
	column.add_child(columns)

	var left := VBoxContainer.new()
	left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left.size_flags_stretch_ratio = 2.0
	left.add_theme_constant_override("separation", 4)
	left.add_child(Style.heading("BUILDINGS", Style.SIZE_HEAD))
	columns.add_child(left)

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	left.add_child(scroll)

	_rows = VBoxContainer.new()
	_rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_rows.add_theme_constant_override("separation", 3)
	scroll.add_child(_rows)

	for kind in _republic.building_kind_count():
		_rows.add_child(_row(kind))

	columns.add_child(_ways_column())

	column.add_child(Style.divider())
	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", 8)
	var back := Style.button("Back")
	back.pressed.connect(close)
	footer.add_child(back)
	column.add_child(footer)


## Roads, rails, tramway, tunnels — everything laid between two points.
##
## Lit is a *variant of the road* rather than a thing you place, which is the
## whole design: nobody wants to site four hundred lamp posts. Only the grades
## the simulation says may carry lamps get the button, read off the authored
## table rather than decided here.
func _ways_column() -> Control:
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_theme_constant_override("separation", 4)
	box.add_child(Style.heading("WAYS", Style.SIZE_HEAD))
	box.add_child(Style.small(
		"Laid between two points, in your own materials and your own builder-days. "
		+ "Street lighting draws off the grid and is what lets a night shift walk home.",
		Style.INK_FAINT
	))

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	box.add_child(scroll)

	var rows := VBoxContainer.new()
	rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rows.add_theme_constant_override("separation", 3)
	scroll.add_child(rows)

	var names := _republic.grade_names()
	var facts: PackedFloat32Array = _republic.grade_facts()
	var stride := 4
	for grade in names.size():
		var o := grade * stride
		if o + stride > facts.size():
			break
		var line := VBoxContainer.new()
		line.add_theme_constant_override("separation", 2)

		var top := HBoxContainer.new()
		top.add_theme_constant_override("separation", 8)
		var name_label := Style.body(String(names[grade]), Style.INK)
		name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		top.add_child(name_label)
		top.add_child(Style.small(
			"%.0f km/h  ·  %.0f builder-days / km" % [facts[o], facts[o + 1]], Style.INK_FAINT
		))
		line.add_child(top)

		var bottom := HBoxContainer.new()
		bottom.add_theme_constant_override("separation", 6)
		var spacer := Control.new()
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		bottom.add_child(spacer)
		var plain := Style.button("Lay")
		plain.pressed.connect(func(): _pick_way(grade, false))
		bottom.add_child(plain)
		if facts[o + 3] > 0.5:
			var lit := Style.button("Lay with lamps")
			lit.pressed.connect(func(): _pick_way(grade, true))
			bottom.add_child(lit)
		line.add_child(bottom)

		var holder := Style.card(Style.PAPER_RAISED, Style.RULE, 7)
		Style.card_body(holder).add_child(line)
		rows.add_child(holder)
	return box


func _pick_way(grade: int, lamps: bool) -> void:
	visible = false
	chose_way.emit(grade, lamps)


## One building: what it is, what it needs, and the two ways to get it.
func _row(kind: int) -> Control:
	var facts: PackedFloat32Array = _republic.building_kind_facts(kind)
	var workers := int(facts[0]) if facts.size() > 0 else 0
	var days := facts[1] if facts.size() > 1 else 0.0
	var contract_cost := facts[2] if facts.size() > 2 else 0.0
	var width := facts[3] if facts.size() > 3 else 0.0
	var depth := facts[4] if facts.size() > 4 else 0.0

	# **Two lines, and the widths expand rather than being pinned.** Fixed
	# minimums of 240 and 320 were fine while this was the only column on the
	# screen; beside the ways list they made the row demand more than its half
	# and pushed the whole right-hand column off the edge — visible in a
	# rendered frame and in nothing else.
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 2)

	var top := HBoxContainer.new()
	top.add_theme_constant_override("separation", 8)
	var name_label := Style.body(String(_republic.building_kind_name(kind)), Style.INK)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top.add_child(name_label)
	var detail := Style.small(
		"%.0f x %.0f m  ·  %d workers  ·  %.0f builder-days" % [width, depth, workers, days],
		Style.INK_FAINT
	)
	detail.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	top.add_child(detail)
	stack.add_child(top)

	var bottom := HBoxContainer.new()
	bottom.add_theme_constant_override("separation", 6)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	bottom.add_child(spacer)

	# **Always offered, and that is deliberate.** `Command::Place` is not refused
	# when the republic has no crews: it puts down a foundation that waits, and
	# work starts the day an office has somebody in it. Placing a site early is
	# therefore a real thing a player might mean to do — queueing the work — and
	# an earlier version of this hid the button until there were builders, which
	# took that away to fix a problem it did not have.
	#
	# The problem it *does* have is silence: pressing this on an empty map looks
	# like nothing happened, because visibly nothing does. That is answered by
	# the crew line under the heading rather than by removing the control.
	var own := Style.button("Build")
	own.pressed.connect(func(): _pick(kind, -1))
	bottom.add_child(own)

	# East only for now: a republic starts with roubles and no dollars, so a
	# Western contract is a button that can only ever refuse until something has
	# been exported. It appears when there is hard currency to spend.
	var east := Style.button("Contract  %s ₽" % _thousands(contract_cost))
	east.pressed.connect(func(): _pick(kind, 0))
	bottom.add_child(east)
	stack.add_child(bottom)

	# `card` gives the panel, `card_body` the padded slot inside it.
	var holder := Style.card(Style.PAPER_RAISED, Style.RULE, 7)
	Style.card_body(holder).add_child(stack)
	return holder


func _pick(kind: int, market: int) -> void:
	visible = false
	chose.emit(kind, market)


## Say whether there is anybody to send, because pressing Build with nobody
## looks exactly like pressing a broken button.
##
## The site is real either way — it waits, and work starts the day an office has
## somebody in it — so this is a sentence rather than a disabled control.
func _refresh_own() -> void:
	if _crews == null:
		return
	var crews: int = _republic.builders() if _republic != null else 0
	if crews > 0:
		_crews.text = "%d builders on the books." % crews
		_crews.add_theme_color_override("font_color", Style.INK_DIM)
	else:
		_crews.text = (
			"No builders yet. Anything you Build now is a foundation that waits "
			+ "until a Construction Office has somebody in it — contracting is what "
			+ "puts a building up today."
		)
		_crews.add_theme_color_override("font_color", Style.ALARM)


func _refresh_purse() -> void:
	if _purse == null or _republic == null:
		return
	var money: PackedFloat32Array = _republic.purse()
	if money.size() < 2:
		return
	_purse.text = "%s ₽   ·   %s $" % [_thousands(money[0]), _thousands(money[1])]


func _thousands(value: float) -> String:
	var digits := str(int(round(value)))
	var out := ""
	for i in digits.length():
		if i > 0 and (digits.length() - i) % 3 == 0:
			out += " "
		out += digits[i]
	return out
