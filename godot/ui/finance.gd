extends CanvasLayer

## The treasury: what the republic holds, what it owes, and who will still lend
## to it.
##
## **A republic could not borrow, repay, or see a debt.** `TakeLoan` and
## `RepayLoan` were bound to nothing and seven views were read by nobody — which
## on a map that starts blank is not a missing convenience. Currency only enters
## at the border, through exports; the industry that earns dollars is built from
## materials bought with dollars; and an advance is the only way out of that
## circle. Without this screen the opening of every republic is decided by the
## rouble grant and nothing else.
##
## # What a screen about money must not become
##
## Nothing domestic costs money in this game, and this screen is not allowed to
## suggest otherwise. There is no "spend roubles to finish the factory" here and
## there will not be one: buildings cost materials and the labour of citizens.
## What money buys is what a foreigner does — goods at the border, a firm to raise
## a building, a builder hired abroad — and every figure on this screen is one of
## those.
##
## # The terms are fixed when the money is taken
##
## Simple interest, locked at borrowing, exactly as a tender's price is locked
## when it is offered. So the ladder shows what a rung *would* cost before it is
## taken and what an advance already taken *does* cost after — and those are two
## different questions, which is why they are two tables.
##
## # A default is not a fine, it is a lost creditor
##
## The treasury refuses to go negative, so a fine levied on an empty purse takes
## nothing. What a default actually costs is the bloc: it never lends again, and
## it prices every trade with you worse from then on. That is stated on this
## screen rather than discovered, because it is the one consequence that does not
## need money to bite.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal closed

const OWING_COLUMNS := [
	["advance", 2.4],
	["outstanding", 1.6, HORIZONTAL_ALIGNMENT_RIGHT],
	["due", 1.2, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.4, HORIZONTAL_ALIGNMENT_RIGHT],
]
const OFFER_COLUMNS := [
	["advance", 1.6, HORIZONTAL_ALIGNMENT_RIGHT],
	["you repay", 1.6, HORIZONTAL_ALIGNMENT_RIGHT],
	["term", 1.2, HORIZONTAL_ALIGNMENT_RIGHT],
	["", 2.6, HORIZONTAL_ALIGNMENT_RIGHT],
]

## The layout of `views::loans`.
const L_BLOC := 0
const L_PRINCIPAL := 1
const L_OWED := 2
const L_REPAID := 3
const L_DAYS := 4
const L_STRIDE := 5

## The layout of `views::loan_tiers`.
const T_PRINCIPAL := 0
const T_INTEREST := 1
const T_TERM := 2
const T_TOTAL := 3
const T_STRIDE := 4

const BLOC_WORDS := ["Eastern Bloc", "Western Alliance"]
const BLOC_MONEY := ["₽", "$"]

var _republic: Republic = null
var _notice: Label = null
var _purse: Label = null
var _record: Label = null
var _owing: VBoxContainer = null
var _offers: VBoxContainer = null
var _built := false


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
		"Finance",
		"Ministry of Finance",
		"Nothing your own people build costs money. What money buys is what a "
		+ "foreigner does: goods at the border, a firm to raise a building, a "
		+ "builder hired abroad. An advance is a bet that you can earn it back."
	)
	var body: VBoxContainer = sheet["body"]
	_notice = sheet["notice"]

	_purse = Parts.say("", "FigureBig")
	body.add_child(_purse)
	_record = Parts.say("", "Faint")
	body.add_child(_record)

	var columns := Parts.columns(body)

	var left := Parts.section(
		columns,
		"What you owe",
		"The whole sum was fixed when the money was taken. Pay it down when you "
		+ "can spare it; the day comes whether the industry arrived or not.",
		2.2
	)
	Parts.head(left, OWING_COLUMNS)
	_owing = Parts.scroller(left)

	var right := Parts.section(
		columns,
		"What the blocs will advance",
		"One advance per bloc at a time. A bigger rung costs more interest, "
		+ "because it is a bigger bet on a republic that has not exported "
		+ "anything yet.",
		2.2
	)
	Parts.head(right, OFFER_COLUMNS)
	_offers = Parts.scroller(right)

	Sheet.close_button(sheet["footer"], "BACK", close)


# ---- what is owed ------------------------------------------------------------


func _rebuild_owing(loans: PackedFloat32Array) -> void:
	for child in _owing.get_children():
		child.queue_free()

	@warning_ignore("integer_division")
	var count: int = loans.size() / L_STRIDE
	if count == 0:
		_owing.add_child(Parts.prose("Nothing is owed to anybody.", "Faint"))
		return

	for i in count:
		var o := i * L_STRIDE
		var bloc := int(loans[o + L_BLOC])
		var outstanding := loans[o + L_OWED] - loans[o + L_REPAID]
		var days := int(loans[o + L_DAYS])
		var line := Parts.row(_owing, i % 2 == 1)

		line.add_child(Parts.cell(Parts.say("%s  ·  %s %s advanced" % [
			BLOC_WORDS[bloc],
			Parts.thousands(loans[o + L_PRINCIPAL]),
			BLOC_MONEY[bloc],
		], "Small"), OWING_COLUMNS[0][1]))

		# Both halves, because a republic that has paid two thirds of a bad
		# advance is in a different position from one that has paid none of a
		# good one, and one number cannot say which this is.
		line.add_child(Parts.cell(Parts.figure("%s of %s %s" % [
			Parts.thousands(outstanding),
			Parts.thousands(loans[o + L_OWED]),
			BLOC_MONEY[bloc],
		]), OWING_COLUMNS[1][1], HORIZONTAL_ALIGNMENT_RIGHT))

		var clock := Parts.figure("%d days" % days if days > 0 else "today")
		clock.add_theme_color_override(
			"font_color", P.ALARM if days < 60 else P.PAPER
		)
		line.add_child(Parts.cell(
			clock, OWING_COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT
		))

		var actions := HBoxContainer.new()
		actions.alignment = BoxContainer.ALIGNMENT_END
		actions.add_theme_constant_override("separation", P.GAP_TIGHT)
		# A part payment and the whole thing, because both are real intentions:
		# clearing it early stops the clock, and paying a slice is how a republic
		# works toward a deadline rather than meeting a cliff.
		var part := Parts.button("PAY A TENTH", "Quiet")
		var slice := loans[o + L_OWED] * 0.1
		part.pressed.connect(func(): _on_repay(bloc, slice))
		actions.add_child(part)
		var all := Parts.button("CLEAR IT", "Primary")
		var whole := outstanding
		all.pressed.connect(func(): _on_repay(bloc, whole))
		actions.add_child(all)
		line.add_child(Parts.cell(actions, OWING_COLUMNS[3][1]))


func _on_repay(bloc: int, amount: float) -> void:
	_say(_republic.repay_loan(bloc, amount))
	refresh()


# ---- what may be borrowed ----------------------------------------------------


## The ladder, once per bloc.
##
## Both blocs on one list rather than a bloc picker, because which one you borrow
## from is a decision about which currency you will have to earn back — and a
## picker hides the comparison the player is actually making.
func _rebuild_offers(tiers: PackedFloat32Array) -> void:
	for child in _offers.get_children():
		child.queue_free()

	@warning_ignore("integer_division")
	var rungs: int = tiers.size() / T_STRIDE
	for bloc in BLOC_WORDS.size():
		_offers.add_child(Parts.gap(P.GAP_TIGHT))
		var head := Parts.say(
			"%s  ·  %s" % [BLOC_WORDS[bloc].to_upper(), BLOC_MONEY[bloc]], "Stamp"
		)
		_offers.add_child(head)
		_offers.add_child(Parts.rule())

		for rung in rungs:
			var o := rung * T_STRIDE
			var line := Parts.row(_offers, rung % 2 == 1)
			line.add_child(Parts.cell(Parts.figure("%s %s" % [
				Parts.thousands(tiers[o + T_PRINCIPAL]), BLOC_MONEY[bloc],
			]), OFFER_COLUMNS[0][1], HORIZONTAL_ALIGNMENT_RIGHT))
			# The total, worked out by the simulation rather than here: it is
			# balance, and it is what the player is deciding against.
			line.add_child(Parts.cell(Parts.figure("%s  (+%d%%)" % [
				Parts.thousands(tiers[o + T_TOTAL]),
				int(round(tiers[o + T_INTEREST] * 100.0)),
			]), OFFER_COLUMNS[1][1], HORIZONTAL_ALIGNMENT_RIGHT))
			line.add_child(Parts.cell(
				Parts.figure("%d days" % int(tiers[o + T_TERM])),
				OFFER_COLUMNS[2][1],
				HORIZONTAL_ALIGNMENT_RIGHT
			))

			var actions := HBoxContainer.new()
			actions.alignment = BoxContainer.ALIGNMENT_END
			# **Asked before it is offered, and the answer is a sentence.**
			# `can_take_loan` puts exactly the question `take_loan` will ask, so
			# a rung that would be refused says why — this bloc is already owed,
			# or it has been defaulted on and is done with you — rather than
			# offering a button that only ever refuses.
			var why: String = _republic.can_take_loan(bloc, rung)
			if why == "":
				# **Not `Primary`, and that came out of looking at the frame.**
				# Six rungs meant six red buttons down the right of the screen,
				# and the palette has exactly one shout in it — red means "this
				# is the decision", which is meaningless when every row claims to
				# be it. The decision here is *which* rung, not that there is one.
				var take := Parts.button("TAKE IT")
				var tier := rung
				var market := bloc
				take.pressed.connect(func(): _on_take(market, tier))
				actions.add_child(take)
			else:
				var said := Parts.say(why, "Faint")
				said.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
				actions.add_child(said)
			line.add_child(Parts.cell(actions, OFFER_COLUMNS[3][1]))


func _on_take(bloc: int, tier: int) -> void:
	_say(_republic.take_loan(bloc, tier))
	refresh()


# ---- the refresh -------------------------------------------------------------


func refresh() -> void:
	if _republic == null or _owing == null:
		return
	var money: PackedFloat32Array = _republic.purse()
	var parts := PackedStringArray()
	for bloc in BLOC_WORDS.size():
		parts.append("%s %s" % [
			Parts.thousands(money[bloc] if bloc < money.size() else 0.0),
			BLOC_MONEY[bloc],
		])
	# What is owed sits beside what is held, because those two figures are only
	# meaningful against each other: eight thousand roubles is a healthy treasury
	# or a republic three months from a default depending on the second number.
	var owed := PackedStringArray()
	for bloc in BLOC_WORDS.size():
		var outstanding: float = _republic.owed_to(bloc)
		if outstanding > 0.5:
			owed.append("%s %s owed" % [Parts.thousands(outstanding), BLOC_MONEY[bloc]])
	_purse.text = "      ".join(parts) + (
		"      ·      %s" % "  ·  ".join(owed) if owed.size() > 0 else ""
	)

	var cleared: int = _republic.loans_cleared()
	var defaulted: int = _republic.loans_defaulted()
	if cleared == 0 and defaulted == 0:
		_record.text = "This republic has never borrowed."
	else:
		_record.text = "%d advance%s repaid, %d defaulted on." % [
			cleared, "" if cleared == 1 else "s", defaulted,
		]
	_record.add_theme_color_override(
		"font_color", P.ALARM if defaulted > 0 else P.PAPER_FAINT
	)

	_rebuild_owing(_republic.loans())
	_rebuild_offers(_republic.loan_tiers())


func _say(why: String) -> void:
	_notice.text = why
