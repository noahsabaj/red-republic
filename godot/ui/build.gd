extends CanvasLayer

## The build menu: what a republic can put up, and what it costs to have it put up.
##
## **This is the screen the game could not be played without.** The shell exposed
## `place()` for five milestones and nothing ever called it — there was no way to
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
## Both prices sit in the same row for exactly that reason: the opening is a
## question of what you can afford to have built for you, and the moment an office
## exists the answer changes.
##
## # Built once, shown many times
##
## A hundred-odd rows is several hundred `Label`s, and building one costs 165x
## what updating it does — measured. So the list is constructed on the first open
## and afterwards only ever shown and hidden. Rebuilding it per open would be a
## visible hitch every single time.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

## Emitted when the player picks something to place. `market` is -1 to build it
## with your own crews, or 0 East / 1 West to contract it out.
signal chose(kind: int, market: int)
## Emitted when the player picks a way to lay. `lamps` asks for street lighting,
## which only paved road may carry.
signal chose_way(grade: int, lamps: bool)
## Emitted when the player picks a kind of line to string, by its place in
## `Utility::ALL`.
signal chose_line(kind: int)
signal closed

## The two tables' columns. Head and rows read the same arrays, which is what
## keeps a column name over the figures it names.
const BUILDING_COLUMNS := [
	["building", 3.0],
	["ground", 1.6, HORIZONTAL_ALIGNMENT_RIGHT],
	["posts", 0.9, HORIZONTAL_ALIGNMENT_RIGHT],
	["builder-days", 1.4, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.6, HORIZONTAL_ALIGNMENT_RIGHT],
]
const WAY_COLUMNS := [
	["way", 2.2],
	["limit", 1.2, HORIZONTAL_ALIGNMENT_RIGHT],
	["days / km", 1.3, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.4, HORIZONTAL_ALIGNMENT_RIGHT],
]
const LINE_COLUMNS := [
	["line", 1.9],
	["reach", 1.0, HORIZONTAL_ALIGNMENT_RIGHT],
	["days / km", 1.1, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.2, HORIZONTAL_ALIGNMENT_RIGHT],
]

## The layout of `tables::utility_table`.
const U_LABOUR := 0
const U_REACH := 1
const U_LOSS := 2
const U_THROUGHPUT := 3
const U_CARRIES := 4

## How wide the two action buttons are held at, so that a column of them is a
## column. Both are as wide as their longest label needs and no wider.
const CONTRACT_WIDTH := 168
const LAMPS_WIDTH := 128
const LAY_WIDTH := 64
const STRING_WIDTH := 80

var _republic: Republic = null
var _purse: Label = null
var _crews: Label = null
var _built := false


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
	var sheet: Dictionary = Sheet.build(
		self,
		"Build",
		"State Committee for Construction",
		"Build it yourself with your own crews and materials, which costs no money and "
		+ "takes as long as you have hands for — or pay a foreign firm, which needs "
		+ "neither and starts the same day."
	)
	var body: VBoxContainer = sheet["body"]
	_crews = sheet["notice"]

	# The purse, in the one voice figures are set in, right under the title block.
	# It is the number every decision on this screen is against.
	_purse = Parts.say("", "FigureBig")
	body.add_child(_purse)

	# **Two tables: things that stand still, and things that go somewhere.** A road
	# was as unbuildable from this game as a factory was before this menu existed,
	# and on a map that starts empty a republic that cannot lay road cannot connect
	# anything it builds.
	var columns := Parts.columns(body)

	var left := Parts.section(
		columns,
		"Buildings",
		"A site is a bill of materials and a place your lorries deliver to. Nothing is "
		+ "finished by paying for it.",
		2.2
	)
	Parts.head(left, BUILDING_COLUMNS)
	var buildings := Parts.scroller(left)
	var facts_of := func(kind: int) -> PackedFloat32Array:
		return _republic.building_kind_facts(kind)
	for kind in _republic.building_kind_count():
		_building_row(buildings, kind, facts_of.call(kind), kind % 2 == 1)

	var middle := Parts.section(
		columns,
		"Ways",
		"Laid between two points, in your own materials and your own builder-days. "
		+ "Street lighting draws off the grid and is what lets a night shift walk home.",
		1.4
	)
	Parts.head(middle, WAY_COLUMNS)
	_way_rows(Parts.scroller(middle))
	middle.add_child(Parts.gap(P.GAP_TIGHT))
	middle.add_child(Parts.prose(_lamp_bill(), "Faint"))

	# **A third column rather than a second table under the ways**, and that is
	# a layout decision taken off a rendered frame: stacked, each list had room
	# for two rows of six and the republic's whole choice of way and line was
	# below a fold in a panel a metre wide.
	#
	# Lines were unreachable in exactly the way ways had been. `order_line` was
	# on the shell and no `.gd` file called it, so a republic could raise a power
	# station and never run a wire out of it — the same shape `place` and
	# `order_way` were in before this screen existed.
	var right := Parts.section(
		columns,
		"Lines",
		"A plant lights only what it is strung to and a boiler warms only what a "
		+ "main runs past. Reach is how far a building may stand from the line and "
		+ "still be on it; loss is what the length of the whole network costs you.",
		1.5
	)
	Parts.head(right, LINE_COLUMNS)
	_line_rows(Parts.scroller(right))

	Sheet.close_button(sheet["footer"], "BACK", close)


## One building: what it is, what it takes, and the two ways to get it.
func _building_row(
	into: VBoxContainer, kind: int, facts: PackedFloat32Array, alt: bool
) -> void:
	var workers := int(facts[0]) if facts.size() > 0 else 0
	var days := facts[1] if facts.size() > 1 else 0.0
	var contract_cost := facts[2] if facts.size() > 2 else 0.0
	var width := facts[3] if facts.size() > 3 else 0.0
	var depth := facts[4] if facts.size() > 4 else 0.0

	var line := Parts.row(into, alt)
	line.add_child(Parts.cell(
		Parts.say(String(_republic.building_kind_name(kind)), "Small"), BUILDING_COLUMNS[0][1]
	))
	line.add_child(Parts.cell(
		Parts.figure("%.0f x %.0f m" % [width, depth]),
		BUILDING_COLUMNS[1][1],
		HORIZONTAL_ALIGNMENT_RIGHT
	))
	line.add_child(Parts.cell(
		Parts.figure(str(workers) if workers > 0 else "—"),
		BUILDING_COLUMNS[2][1],
		HORIZONTAL_ALIGNMENT_RIGHT
	))
	line.add_child(Parts.cell(
		Parts.figure("%.0f" % days), BUILDING_COLUMNS[3][1], HORIZONTAL_ALIGNMENT_RIGHT
	))

	var actions := HBoxContainer.new()
	actions.alignment = BoxContainer.ALIGNMENT_END
	actions.add_theme_constant_override("separation", P.GAP_TIGHT)

	# **Always offered, and that is deliberate.** `Command::Place` is not refused
	# when the republic has no crews: it puts down a foundation that waits, and
	# work starts the day an office has somebody in it. Placing a site early is a
	# real thing a player might mean to do — queueing the work — and an earlier
	# version hid the button until there were builders, which took that away to fix
	# a problem it did not have. What it *does* have is silence, and that is
	# answered by the notice line rather than by removing the control.
	var own := Parts.button("BUILD")
	own.pressed.connect(func(): _pick(kind, -1))
	actions.add_child(own)

	# East only for now: a republic starts with roubles and no dollars, so a
	# Western contract is a button that can only ever refuse until something has
	# been exported.
	var east := Parts.button("CONTRACT  %s ₽" % Parts.thousands(contract_cost))
	# **A fixed width, so the two buttons are two columns.** The price is part of
	# the label and a five-digit one is narrower than a six-digit one, so without
	# this every Build button in the list sat at a slightly different place --
	# which is the one thing a table is for.
	east.custom_minimum_size = Vector2(CONTRACT_WIDTH, P.BUTTON_HEIGHT)
	east.pressed.connect(func(): _pick(kind, 0))
	actions.add_child(east)
	line.add_child(Parts.cell(actions, BUILDING_COLUMNS[4][1]))


## Roads, rails, tramway, bridges — everything laid between two points.
##
## Lit is a *variant of the road* rather than a thing you place, which is the
## whole design: nobody wants to site four hundred lamp posts. Only the grades the
## simulation says may carry lamps get the button, read off the authored table
## rather than decided here.
func _way_rows(into: VBoxContainer) -> void:
	var names := _republic.grade_names()
	var facts: PackedFloat32Array = _republic.grade_facts()
	var stride := 4
	for grade in names.size():
		var o := grade * stride
		if o + stride > facts.size():
			break
		var line := Parts.row(into, grade % 2 == 1)
		line.add_child(Parts.cell(
			Parts.say(String(names[grade]), "Small"), WAY_COLUMNS[0][1]
		))
		line.add_child(Parts.cell(
			Parts.figure("%.0f km/h" % facts[o]), WAY_COLUMNS[1][1], HORIZONTAL_ALIGNMENT_RIGHT
		))
		line.add_child(Parts.cell(
			Parts.figure("%.0f" % facts[o + 1]), WAY_COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT
		))

		var actions := HBoxContainer.new()
		actions.alignment = BoxContainer.ALIGNMENT_END
		actions.add_theme_constant_override("separation", P.GAP_TIGHT)
		# **The lamps button keeps its place whether or not a grade takes lamps.**
		# Only paved road may carry them, so without a reserved slot the Lay button
		# on every other grade sat a hundred pixels right of the one beside it.
		var lit_slot := Control.new()
		lit_slot.custom_minimum_size = Vector2(LAMPS_WIDTH, 0)
		if facts[o + 3] > 0.5:
			var lit := Parts.button("WITH LAMPS")
			lit.custom_minimum_size = Vector2(LAMPS_WIDTH, P.BUTTON_HEIGHT)
			lit.pressed.connect(func(): _pick_way(grade, true))
			actions.add_child(lit)
		else:
			actions.add_child(lit_slot)

		var plain := Parts.button("LAY")
		plain.custom_minimum_size = Vector2(LAY_WIDTH, P.BUTTON_HEIGHT)
		plain.pressed.connect(func(): _pick_way(grade, false))
		actions.add_child(plain)
		line.add_child(Parts.cell(actions, WAY_COLUMNS[3][1]))


## Power, heat, belts, pipelines and trolley wire — everything strung between two
## points.
##
## The same four columns the ways table has, for the same reason: this is a
## chooser, and what is chosen between is reach, cost and what the thing is good
## for. The bill of materials is in the reference beside the grades' own, where a
## list of a different length per row does not have to fit in a column.
func _line_rows(into: VBoxContainer) -> void:
	var names := _republic.utility_names()
	var facts: PackedFloat32Array = _republic.utility_table()
	var stride: int = _republic.utility_stride()
	for kind in names.size():
		var o := kind * stride
		if o + stride > facts.size():
			break
		var line := Parts.row(into, kind % 2 == 1)
		line.add_child(Parts.cell(
			Parts.say(String(names[kind]), "Small"), LINE_COLUMNS[0][1]
		))
		line.add_child(Parts.cell(
			Parts.figure("%.0f m" % facts[o + U_REACH]),
			LINE_COLUMNS[1][1],
			HORIZONTAL_ALIGNMENT_RIGHT
		))
		line.add_child(Parts.cell(
			Parts.figure("%.0f" % facts[o + U_LABOUR]),
			LINE_COLUMNS[2][1],
			HORIZONTAL_ALIGNMENT_RIGHT
		))

		var actions := HBoxContainer.new()
		actions.alignment = BoxContainer.ALIGNMENT_END
		actions.add_theme_constant_override("separation", P.GAP_TIGHT)
		# **What a kind of line is *for* differs, and the difference is the
		# choice.** A belt and a pipeline are a throughput; power and heat are
		# neither, because what they carry is not tonnage — what matters about
		# them is what the span leaks over its length, which is the whole
		# argument for siting a plant near what it serves.
		actions.add_child(Parts.say(
			"%s t / day" % Parts.clean(facts[o + U_THROUGHPUT])
			if facts[o + U_CARRIES] > 0.5
			else "%.1f%% lost / km" % (facts[o + U_LOSS] * 100.0),
			"Faint"
		))
		var string_it := Parts.button("STRING")
		string_it.custom_minimum_size = Vector2(STRING_WIDTH, P.BUTTON_HEIGHT)
		string_it.pressed.connect(func(): _pick_line(kind))
		actions.add_child(string_it)
		line.add_child(Parts.cell(actions, LINE_COLUMNS[3][1]))


func _pick_line(kind: int) -> void:
	visible = false
	chose_line.emit(kind)


## What a kilometre of street lighting costs on top of the road under it.
##
## **One sentence under the table rather than a column, because it is one price.**
## Lamps are a variant of the road and the bill is the same whichever grade
## carries them, so a per-row column would print the identical figures on every
## line that has the button. Without it WITH LAMPS was a button that spent
## builder-days, steel, electronics and megawatts and named none of them --
## which is the one thing a player has to know before pressing it, because the
## draw is permanent and comes off a grid that may not have it.
func _lamp_bill() -> String:
	var bill: PackedFloat32Array = _republic.lamp_cost()
	if bill.size() < 2:
		return ""
	var names: PackedStringArray = _republic.resource_names()
	var goods := PackedStringArray()
	var i := 2
	while i + 1 < bill.size():
		var resource := int(bill[i])
		if resource >= 0 and resource < names.size():
			goods.append("%s t %s" % [Parts.clean(bill[i + 1]), String(names[resource]).to_lower()])
		i += 2
	return "Lamps add %s builder-days and %s to every kilometre, and draw %.2f MW off the grid once they are burning." % [
		Parts.clean(bill[0]), ", ".join(goods), bill[1],
	]


func _pick_way(grade: int, lamps: bool) -> void:
	visible = false
	chose_way.emit(grade, lamps)


func _pick(kind: int, market: int) -> void:
	visible = false
	chose.emit(kind, market)


## Say whether there is anybody to send, because pressing Build with nobody looks
## exactly like pressing a broken button.
##
## The site is real either way — it waits, and work starts the day an office has
## somebody in it — so this is a sentence rather than a disabled control.
func _refresh_own() -> void:
	if _crews == null:
		return
	var crews: int = _republic.builders() if _republic != null else 0
	if crews > 0:
		_crews.text = "%d builders on the books." % crews
		_crews.add_theme_color_override("font_color", P.PAPER_DIM)
	else:
		_crews.text = (
			"No builders yet. Anything you Build now is a foundation that waits until a "
			+ "Construction Office has somebody in it — contracting is what puts a "
			+ "building up today."
		)
		_crews.add_theme_color_override("font_color", P.ALARM)


func _refresh_purse() -> void:
	if _purse == null or _republic == null:
		return
	var money: PackedFloat32Array = _republic.purse()
	if money.size() < 2:
		return
	_purse.text = "%s ₽      %s $" % [
		Parts.thousands(money[0]), Parts.thousands(money[1]),
	]
