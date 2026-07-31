extends CanvasLayer

## The export plan: standing instructions to the customs houses, and the tenders
## the blocs put on the table.
##
## **The whole reason a customs house exists, and it was unreachable.**
## `AddTradeRule`, `MoveTradeRule` and `RemoveTradeRule` were on the simulation
## and bound to nothing; a republic could build a customs house, haul coal to it,
## and have no way to say what to do with the coal.
##
## # A list, in the player's own order
##
## The order is the decision. A customs house clears only so many tonnes a day and
## the treasury holds only so much hard currency, so when either runs short the
## first rule is served first — which is why moving a rule up the list is its own
## command in the simulation rather than a re-send of the whole policy. The arrows
## on each row are that command, and they are the sharpest control on this screen.
##
## # Selling and buying are not symmetrical, and the form says so
##
## A **sell** rule is unconditional: whatever reaches the customs house goes. A
## **buy** rule is a level to keep — "top this post up to forty tonnes of
## machinery" — because buying without a ceiling is an open invitation to spend
## every dollar the republic has on a good nothing is waiting for.
##
## # Tenders are here rather than with the money
##
## A tender is a bulk order at a premium on a deadline, and it is the same
## business as the export plan: it decides what leaves the country. What it costs
## when it goes wrong is on the finance screen beside the advances, because that
## is a question about the treasury.
##
## Both halves read the border's own prices through the simulation, and neither
## works one out. A panel that priced a tonne itself would be a second copy of the
## balance and only one of them is tested.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal closed

const RULE_COLUMNS := [
	["goods", 2.2],
	["with", 1.8],
	["instruction", 2.0, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.0, HORIZONTAL_ALIGNMENT_RIGHT],
]
const TENDER_COLUMNS := [
	["tender", 3.0],
	["delivered", 1.6, HORIZONTAL_ALIGNMENT_RIGHT],
	["due", 1.2, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.0, HORIZONTAL_ALIGNMENT_RIGHT],
]

## The layout of `views::trade_rules`.
const RULE_RESOURCE := 0
const RULE_BLOC := 1
const RULE_ACTION := 2
const RULE_UP_TO := 3
const RULE_STRIDE := 4

## The layout of `views::contracts`.
const C_ID := 0
const C_RESOURCE := 1
const C_BLOC := 2
const C_TONNES := 3
const C_DELIVERED := 4
const C_PRICE := 5
const C_DAYS := 6
const C_ANSWER_BY := 7
const C_STATE := 8
const C_FINE := 9
const C_STRIDE := 10

## What each state of a tender is called, in the order `views::contracts`
## reports. Checked against `contract_states()` by `check`, because a list of
## words beside a simulation roster is the copy that silently loses a row.
const STATE_WORDS := ["on the table", "running", "delivered", "failed"]

## What the two blocs are called, in `Market::ALL` order.
const BLOC_WORDS := ["Eastern Bloc", "Western Alliance"]
## Which purse each pays in. A rouble and a dollar are separate money in this
## game, and a rule that earns the wrong one earns nothing you can spend.
const BLOC_MONEY := ["₽", "$"]

var _republic: Republic = null
var _rules: VBoxContainer = null
var _tenders: VBoxContainer = null
var _notice: Label = null
var _summary: Label = null
var _built := false

## What the new-rule row currently says. Held rather than read back off the
## controls, so the one place a value can be wrong is the one place it is stored.
var _draft_resource := 0
var _draft_bloc := 0
var _draft_buy := false
var _draft_up_to := 20.0

var _draft_goods: OptionButton = null
var _draft_market: OptionButton = null
var _draft_action: Button = null
var _draft_level: Label = null


func _ready() -> void:
	layer = 13
	visible = false


func open(republic: Republic) -> void:
	_republic = republic
	if not _built:
		_build()
		_built = true
	refresh()
	visible = true


func close() -> void:
	visible = false
	closed.emit()


func _build() -> void:
	var sheet: Dictionary = Sheet.build(
		self,
		"Trade",
		"Foreign Trade Directorate",
		"Currency enters this republic at the border and nowhere else. These are "
		+ "the standing instructions your customs houses work to, in the order "
		+ "they are served when throughput or hard currency runs short."
	)
	var body: VBoxContainer = sheet["body"]
	_notice = sheet["notice"]

	_summary = Parts.say("", "Figure")
	body.add_child(_summary)

	var columns := Parts.columns(body)

	var left := Parts.section(
		columns,
		"The export plan",
		"Sell sends whatever reaches the post. Buy keeps it topped up to a level, "
		+ "and spends hard currency to do it.",
		2.4
	)
	_build_draft(left)
	Parts.head(left, RULE_COLUMNS)
	_rules = Parts.scroller(left)

	var right := Parts.section(
		columns,
		"Tenders",
		"A fixed tonnage by a fixed day at a price locked when it was offered. "
		+ "Missing one costs a fine and sours the bloc on you.",
		2.0
	)
	Parts.head(right, TENDER_COLUMNS)
	_tenders = Parts.scroller(right)

	Sheet.close_button(sheet["footer"], "BACK", close)


## The row that writes a new rule: what, with whom, and on what terms.
##
## A well above the table rather than a dialog, because adding a rule is the
## thing this screen exists for and a screen whose main verb is behind a button
## that opens a window is a screen with an extra click in it.
func _build_draft(into: VBoxContainer) -> void:
	var well := PanelContainer.new()
	well.theme_type_variation = "Well"
	into.add_child(well)

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", P.GAP)
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	well.add_child(line)

	_draft_goods = OptionButton.new()
	for name in _republic.resource_names():
		_draft_goods.add_item(String(name))
	_draft_goods.item_selected.connect(func(i: int): _draft_resource = i)
	line.add_child(_draft_goods)

	_draft_market = OptionButton.new()
	for bloc in BLOC_WORDS:
		_draft_market.add_item(String(bloc))
	_draft_market.item_selected.connect(func(i: int): _draft_bloc = i)
	line.add_child(_draft_market)

	# Two words rather than a tick box: "sell" and "buy" are opposite
	# instructions, and a box labelled `BUY` that is unticked reads as an
	# instruction to do nothing rather than as the other one.
	_draft_action = Parts.button("SELL", "Quiet")
	_draft_action.custom_minimum_size = Vector2(84, P.BUTTON_HEIGHT)
	_draft_action.pressed.connect(_on_flip_action)
	line.add_child(_draft_action)

	_draft_level = Parts.figure("")
	_draft_level.custom_minimum_size = Vector2(96, 0)
	line.add_child(_draft_level)
	line.add_child(Parts.stepper(func(delta: float): _on_nudge_level(delta), 10.0))

	line.add_child(Parts.fill())
	var add := Parts.button("ADD", "Primary")
	add.pressed.connect(_on_add)
	line.add_child(add)


func _on_flip_action() -> void:
	_draft_buy = not _draft_buy
	_write_draft()


func _on_nudge_level(delta: float) -> void:
	_draft_up_to = maxf(_draft_up_to + delta, 0.0)
	_write_draft()


func _write_draft() -> void:
	_draft_action.text = "BUY" if _draft_buy else "SELL"
	# A ceiling means nothing on a sell rule, so it goes quiet rather than
	# showing a number that decides nothing.
	_draft_level.text = "up to %s t" % Parts.clean(_draft_up_to) if _draft_buy else "—"
	_draft_level.add_theme_color_override(
		"font_color", P.PAPER if _draft_buy else P.PAPER_FAINT
	)


func _on_add() -> void:
	_say(_republic.add_trade_rule(
		_draft_resource, _draft_bloc, _draft_buy, _draft_up_to
	))
	refresh()


# ---- the rules ---------------------------------------------------------------


## Rebuilt whenever the plan changes.
##
## Unlike the workplace table, a rule list is short and every edit reorders it —
## adding, removing and moving all shift the rows under the buttons — so pooling
## rows by index would mean rewriting every one of them anyway. What is pooled is
## the screen: it is built once, and only this list is thrown away.
func _rebuild_rules(rules: PackedFloat32Array) -> void:
	for child in _rules.get_children():
		child.queue_free()

	var names: PackedStringArray = _republic.resource_names()
	@warning_ignore("integer_division")
	var count: int = rules.size() / RULE_STRIDE
	if count == 0:
		_rules.add_child(Parts.prose(
			"No standing instructions. A customs house with no plan clears "
			+ "nothing, whatever your lorries bring it.",
			"Faint"
		))
		return

	for i in count:
		var o := i * RULE_STRIDE
		var line := Parts.row(_rules, i % 2 == 1)
		var resource := int(rules[o + RULE_RESOURCE])
		var bloc := int(rules[o + RULE_BLOC])
		var buying := rules[o + RULE_ACTION] > 0.5

		line.add_child(Parts.cell(
			Parts.say(String(names[resource]) if resource < names.size() else "—", "Small"),
			RULE_COLUMNS[0][1]
		))
		line.add_child(Parts.cell(
			Parts.say("%s  %s" % [BLOC_WORDS[bloc], BLOC_MONEY[bloc]], "Small"),
			RULE_COLUMNS[1][1]
		))
		line.add_child(Parts.cell(
			Parts.figure(
				"buy up to %s t" % Parts.clean(rules[o + RULE_UP_TO]) if buying
				else "sell all of it"
			),
			RULE_COLUMNS[2][1],
			HORIZONTAL_ALIGNMENT_RIGHT
		))

		var actions := HBoxContainer.new()
		actions.alignment = BoxContainer.ALIGNMENT_END
		actions.add_theme_constant_override("separation", P.GAP_TIGHT)
		var index := i
		# Up and down are the ranking, which **is** the decision this screen
		# makes: when the customs house or the treasury runs short the first rule
		# is served first.
		for pair: Array in [["↑", -1], ["↓", 1]]:
			var move := Parts.button(String(pair[0]), "Step")
			move.custom_minimum_size = Vector2(P.STEP_BUTTON, P.STEP_BUTTON)
			var by: int = pair[1]
			move.pressed.connect(func(): _on_move(index, index + by))
			actions.add_child(move)
		var drop := Parts.button("WITHDRAW", "Quiet")
		drop.pressed.connect(func(): _on_remove(index))
		actions.add_child(drop)
		line.add_child(Parts.cell(actions, RULE_COLUMNS[3][1]))


func _on_move(from: int, to: int) -> void:
	# The refusal for an edge move is the simulation's — "there is no trade rule
	# 4; there are 4" — and printing it is better than a button that silently
	# does nothing at the ends of the list.
	_say(_republic.move_trade_rule(from, to))
	refresh()


func _on_remove(index: int) -> void:
	_say(_republic.remove_trade_rule(index))
	refresh()


# ---- the tenders -------------------------------------------------------------


func _rebuild_tenders(tenders: PackedFloat32Array) -> void:
	for child in _tenders.get_children():
		child.queue_free()

	var names: PackedStringArray = _republic.resource_names()
	@warning_ignore("integer_division")
	var count: int = tenders.size() / C_STRIDE
	if count == 0:
		_tenders.add_child(Parts.prose(
			"Nothing on the table. The Directorate offers a tender every other "
			+ "month, once there is a frontier post for it to land at.",
			"Faint"
		))
		return

	for i in count:
		var o := i * C_STRIDE
		var line := Parts.row(_tenders, i % 2 == 1)
		var resource := int(tenders[o + C_RESOURCE])
		var bloc := int(tenders[o + C_BLOC])
		var state := int(tenders[o + C_STATE])
		var offered := state == 0

		var what := Parts.say("%s t %s  ·  %s" % [
			Parts.clean(tenders[o + C_TONNES]),
			String(names[resource]) if resource < names.size() else "—",
			BLOC_WORDS[bloc],
		], "Small")
		line.add_child(Parts.cell(what, TENDER_COLUMNS[0][1]))

		# What the whole tender is worth, at the price locked when it was
		# offered — the number that decides whether it is worth taking.
		var worth := tenders[o + C_TONNES] * tenders[o + C_PRICE]
		var delivered := Parts.figure("%s of %s %s" % [
			Parts.clean(tenders[o + C_DELIVERED]),
			Parts.thousands(worth),
			BLOC_MONEY[bloc],
		])
		line.add_child(Parts.cell(
			delivered, TENDER_COLUMNS[1][1], HORIZONTAL_ALIGNMENT_RIGHT
		))

		# An offer is counted down to the day it is withdrawn; a running tender
		# to the day it is due. They are different clocks and showing the wrong
		# one is how a player misses a deadline they were watching.
		var days := int(tenders[o + (C_ANSWER_BY if offered else C_DAYS)])
		var clock := Parts.figure(
			"%d days" % days if days >= 0 else STATE_WORDS[clampi(state, 0, STATE_WORDS.size() - 1)]
		)
		clock.add_theme_color_override(
			"font_color", P.ALARM if days < 14 else P.PAPER
		)
		line.add_child(Parts.cell(
			clock, TENDER_COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT
		))

		var actions := HBoxContainer.new()
		actions.alignment = BoxContainer.ALIGNMENT_END
		actions.add_theme_constant_override("separation", P.GAP_TIGHT)
		if offered:
			var id := int(tenders[o + C_ID])
			var take := Parts.button("ACCEPT", "Primary")
			take.pressed.connect(func(): _on_accept(id))
			actions.add_child(take)
			var refuse := Parts.button("DECLINE", "Quiet")
			refuse.pressed.connect(func(): _on_decline(id))
			actions.add_child(refuse)
		else:
			# A running tender is a debt in goods, and what it costs to miss is
			# the only thing left to say about it.
			var fine := tenders[o + C_FINE]
			var word := Parts.figure(
				"%s %s if missed" % [Parts.thousands(fine), BLOC_MONEY[bloc]] if fine > 0.5
				else STATE_WORDS[clampi(state, 0, STATE_WORDS.size() - 1)]
			)
			word.add_theme_color_override("font_color", P.PAPER_FAINT)
			actions.add_child(word)
		line.add_child(Parts.cell(actions, TENDER_COLUMNS[3][1]))


func _on_accept(id: int) -> void:
	_say(_republic.accept_contract(id))
	refresh()


func _on_decline(id: int) -> void:
	_say(_republic.decline_contract(id))
	refresh()


# ---- the refresh -------------------------------------------------------------


func refresh() -> void:
	if _republic == null or _rules == null:
		return
	_write_draft()
	var rules: PackedFloat32Array = _republic.trade_rules()
	var tenders: PackedFloat32Array = _republic.contracts()
	_rebuild_rules(rules)
	_rebuild_tenders(tenders)

	var money: PackedFloat32Array = _republic.purse()
	var relations: PackedFloat32Array = _republic.bloc_relations()
	var parts := PackedStringArray()
	for bloc in BLOC_WORDS.size():
		var sour := relations[bloc] if bloc < relations.size() else 0.0
		parts.append("%s %s%s" % [
			Parts.thousands(money[bloc] if bloc < money.size() else 0.0),
			BLOC_MONEY[bloc],
			# Only when there is one. A permanent "relations 0%" would read as a
			# score rather than as a mark the republic earned and will lose.
			"  (%s sour by %d%%)" % [BLOC_WORDS[bloc], int(round(sour * 100.0))] if sour > 0.005
			else "",
		])
	_summary.text = "      ".join(parts)


func _say(why: String) -> void:
	_notice.text = why


## Check the words this screen keeps against the roster they name.
##
## Called by `--check`. `cargo test` cannot see GDScript, so the one table here
## that shadows a simulation roster is compared where GDScript runs.
func check(republic: Republic) -> String:
	if STATE_WORDS.size() != republic.contract_states():
		return "the trade screen names %d tender states and the simulation has %d" % [
			STATE_WORDS.size(), republic.contract_states(),
		]
	return ""
