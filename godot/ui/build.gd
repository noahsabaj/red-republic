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
signal closed

var _republic: Node = null
var _rows: VBoxContainer = null
var _purse: Label = null
var _built := false


func _ready() -> void:
	layer = 12
	visible = false


func open(republic: Node) -> void:
	_republic = republic
	if not _built:
		_build()
		_built = true
	_refresh_purse()
	visible = true


func close() -> void:
	visible = false
	closed.emit()


func _build() -> void:
	# Near-opaque. At 0.86 the HUD read straight through a full-screen list and
	# the whole thing looked like two screens printed on top of each other.
	add_child(Style.backdrop(0.97))

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
	column.add_child(Style.small(
		"Build it yourself with your own crews and materials, which costs no money — "
		+ "or pay a foreign firm, which needs neither and is the only thing that works "
		+ "before you have a Construction Office.",
		Style.INK_DIM
	))
	column.add_child(Style.divider())

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(scroll)

	_rows = VBoxContainer.new()
	_rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_rows.add_theme_constant_override("separation", 3)
	scroll.add_child(_rows)

	for kind in _republic.building_kind_count():
		_rows.add_child(_row(kind))

	column.add_child(Style.divider())
	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", 8)
	var back := Style.button("Back")
	back.pressed.connect(close)
	footer.add_child(back)
	column.add_child(footer)


## One building: what it is, what it needs, and the two ways to get it.
func _row(kind: int) -> Control:
	var facts: PackedFloat32Array = _republic.building_kind_facts(kind)
	var workers := int(facts[0]) if facts.size() > 0 else 0
	var days := facts[1] if facts.size() > 1 else 0.0
	var contract_cost := facts[2] if facts.size() > 2 else 0.0
	var width := facts[3] if facts.size() > 3 else 0.0
	var depth := facts[4] if facts.size() > 4 else 0.0

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 12)

	var name_label := Style.body(String(_republic.building_kind_name(kind)), Style.INK)
	name_label.custom_minimum_size = Vector2(240, 0)
	line.add_child(name_label)

	var detail := Style.small(
		"%.0f x %.0f m  ·  %d workers  ·  %.0f builder-days" % [width, depth, workers, days],
		Style.INK_FAINT
	)
	detail.custom_minimum_size = Vector2(320, 0)
	line.add_child(detail)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(spacer)

	var own := Style.button("Build")
	own.pressed.connect(func(): _pick(kind, -1))
	line.add_child(own)

	# East only for now: a republic starts with roubles and no dollars, so a
	# Western contract is a button that can only ever refuse until something has
	# been exported. It appears when there is hard currency to spend.
	var east := Style.button("Contract  %s ₽" % _thousands(contract_cost))
	east.pressed.connect(func(): _pick(kind, 0))
	line.add_child(east)

	# `card` gives the panel, `card_body` the padded slot inside it.
	var holder := Style.card(Style.PAPER_RAISED, Style.RULE, 7)
	Style.card_body(holder).add_child(line)
	return holder


func _pick(kind: int, market: int) -> void:
	visible = false
	chose.emit(kind, market)


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
