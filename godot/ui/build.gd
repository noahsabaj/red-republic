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

	var right := Parts.section(
		columns,
		"Ways",
		"Laid between two points, in your own materials and your own builder-days. "
		+ "Street lighting draws off the grid and is what lets a night shift walk home.",
		1.5
	)
	Parts.head(right, WAY_COLUMNS)
	var ways := Parts.scroller(right)
	_way_rows(ways)

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
		var plain := Parts.button("LAY")
		plain.pressed.connect(func(): _pick_way(grade, false))
		actions.add_child(plain)
		if facts[o + 3] > 0.5:
			var lit := Parts.button("WITH LAMPS")
			lit.pressed.connect(func(): _pick_way(grade, true))
			actions.add_child(lit)
		line.add_child(Parts.cell(actions, WAY_COLUMNS[3][1]))


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
