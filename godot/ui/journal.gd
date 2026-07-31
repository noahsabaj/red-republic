extends CanvasLayer

## Everything the player has ever ordered, in the order they ordered it.
##
## **The record already existed and nobody could read it.** Every accepted command
## travels inside the save — that is what makes a save a record of how its republic
## came to be rather than only what it currently is, and what makes any reported
## bug reproducible from the save alone. `journal_len` and the entries behind it
## were read by nothing at all.
##
## Refused commands are not in here, and that is not an omission: a refusal changed
## nothing, so replaying it would be replaying a no-op. What this shows is exactly
## the set of things that moved the world.
##
## # The sentences are composed here, and that is the whole point
##
## The simulation hands over a verb and up to six figures. Turning
## `(1, kind 12, East, 940, 2160)` into *"contracted a Sawmill from the Eastern
## Bloc"* is the interface's work, and the interface is Godot's — which is why the
## binding this replaced, a `format!` in Rust that printed `{:?}` on a command
## enum, had to go. A player was going to be shown `SetStandingOrder { building:
## BuildingId(14), resource: Coal, tonnes: Tonnes(40.0) }`.
##
## [`PHRASES`] is the one table in this project that is a list of words beside a
## simulation enum with no other way to write it. It is checked against
## `journal_verbs()` on open and by `--check`, because a phrase table that is one
## row short prints a blank line — which reads as a command that did nothing.
##
## # Windowed, because a decade of play is not a list
##
## A republic run for ten years has tens of thousands of entries. The screen reads
## a page at a time through `journal_page`, newest page first, and pages back
## through them — so the cost of opening this is the same on day one and on day
## four thousand.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal closed

const COLUMNS := [
	["day", 0.8, HORIZONTAL_ALIGNMENT_RIGHT],
	["what was ordered", 5.4],
	["where", 1.6, HORIZONTAL_ALIGNMENT_RIGHT],
]

## How many entries a page holds. Enough to fill the sheet without scrolling
## past the fold on the shipped resolution.
const PAGE := 26

## The layout of `views::journal`.
const J_TICK := 0
const J_DAY := 1
const J_VERB := 2
const J_A := 3
const J_B := 4
const J_C := 5
const J_D := 6
const J_E := 7
const J_F := 8
const J_STRIDE := 9

## What each verb is called, in the order `views::verb_of` assigns.
##
## **The order is the simulation's and the words are this file's.** Adding a
## `Command` variant breaks the Rust match that assigns these indices, which is
## what brings somebody here — and if they do not come, `check` fails rather than
## a row printing blank.
const PHRASES := [
	"commissioned",           # 0  Place
	"contracted",             # 1  ContractBuild
	"pulled down",            # 2  Demolish
	"ordered",                # 3  OrderRoad
	"strung",                 # 4  OrderLine
	"recalled the crew from", # 5  RecallCrew
	"set imports",            # 6  SetImportPolicy
	"dropped the import rule for",  # 7  ClearImportPolicy
	"told",                   # 8  SetStandingOrder
	"hired abroad",           # 9  HireForeign
	"accepted a tender",      # 10 AcceptContract
	"declined a tender",      # 11 DeclineContract
	"added a trade rule",     # 12 AddTradeRule
	"withdrew a trade rule",  # 13 RemoveTradeRule
	"moved a trade rule",     # 14 MoveTradeRule
	"took an advance",        # 15 TakeLoan
	"repaid an advance",      # 16 RepayLoan
	"set the working day",    # 17 SetNationalShiftHours
	"set the hours",          # 18 SetShiftHours
	"rostered",               # 19 SetShifts
	"ranked",                 # 20 SetPriority
	"named the republic",     # 21 NameRepublic
]

const BLOC_WORDS := ["Eastern Bloc", "Western Alliance"]

var _republic: Republic = null
var _rows: VBoxContainer = null
var _summary: Label = null
var _page := 0
var _built := false


func _ready() -> void:
	layer = 13
	visible = false


func open(republic: Republic) -> void:
	_republic = republic
	if not _built:
		_build()
		_built = true
	# Opened at the end, because what a player wants from a log is what just
	# happened. Paging goes backwards from here.
	_page = _last_page()
	refresh()
	visible = true


func close() -> void:
	visible = false
	closed.emit()


@warning_ignore("integer_division")
func _last_page() -> int:
	var total: int = _republic.journal_len()
	return maxi(0, (total - 1) / PAGE) if total > 0 else 0


func _build() -> void:
	var sheet: Dictionary = Sheet.build(
		self,
		"The Journal",
		"Central Archive",
		"Every order this republic has carried out, in the order it carried them "
		+ "out. It travels inside the save, so a republic can always be replayed "
		+ "from its own record."
	)
	# No notice line is kept: nothing on this screen can be refused. It is a
	# record, and paging through it asks the republic for nothing.
	var body: VBoxContainer = sheet["body"]

	_summary = Parts.say("", "Figure")
	body.add_child(_summary)

	Parts.head(body, COLUMNS)
	_rows = Parts.scroller(body)

	var footer: HBoxContainer = sheet["footer"]
	Sheet.close_button(footer, "BACK", close)
	var back := Parts.button("EARLIER", "Quiet")
	back.pressed.connect(func(): _turn(-1))
	footer.add_child(back)
	var forward := Parts.button("LATER", "Quiet")
	forward.pressed.connect(func(): _turn(1))
	footer.add_child(forward)


func _turn(by: int) -> void:
	_page = clampi(_page + by, 0, _last_page())
	refresh()


# ---- the refresh -------------------------------------------------------------


func refresh() -> void:
	if _republic == null or _rows == null:
		return
	for child in _rows.get_children():
		child.queue_free()

	var total: int = _republic.journal_len()
	if total == 0:
		_summary.text = "Nothing has been ordered yet."
		_rows.add_child(Parts.prose(
			"This republic has been founded and told nothing. Everything you do "
			+ "from here appears on this page.",
			"Faint"
		))
		return

	var from := _page * PAGE
	var entries: PackedFloat32Array = _republic.journal_page(from, PAGE)
	@warning_ignore("integer_division")
	var count: int = entries.size() / J_STRIDE
	_summary.text = "%d orders      showing %d to %d" % [
		total, from + 1, from + count,
	]

	for i in count:
		var o := i * J_STRIDE
		var line := Parts.row(_rows, i % 2 == 1)
		line.add_child(Parts.cell(
			Parts.figure("%d" % int(entries[o + J_DAY])),
			COLUMNS[0][1],
			HORIZONTAL_ALIGNMENT_RIGHT
		))
		line.add_child(Parts.cell(
			Parts.say(_sentence(entries, o, from + i), "Small"), COLUMNS[1][1]
		))
		line.add_child(Parts.cell(
			Parts.figure(_where(entries, o)), COLUMNS[2][1], HORIZONTAL_ALIGNMENT_RIGHT
		))


## Where an order was aimed, or nothing.
##
## Only the verbs that name a point on the map: a place to build, a road, a line.
## A column of coordinates against orders that have none would be a column of
## dashes, and the two verbs that do have them are the ones a player might want to
## go and look at.
func _where(entries: PackedFloat32Array, o: int) -> String:
	match int(entries[o + J_VERB]):
		0, 1:
			return "%.0f, %.0f" % [entries[o + J_C], entries[o + J_D]]
		3, 4:
			return "%.0f, %.0f" % [entries[o + J_C], entries[o + J_D]]
		_:
			return ""


## Turn one entry into a sentence.
##
## Every branch reads names off the rosters — `building_kind_name`,
## `resource_names`, `grade_names`, `utility_names`, `priority_names` — rather
## than keeping a copy of them here. The only words this file owns are the verbs
## in [`PHRASES`] and the two bloc names, which are the parts the simulation has
## no name for outside a refusal.
func _sentence(entries: PackedFloat32Array, o: int, index: int) -> String:
	var verb := int(entries[o + J_VERB])
	var phrase: String = (
		PHRASES[verb] if verb >= 0 and verb < PHRASES.size() else "did something"
	)
	var a := entries[o + J_A]
	var b := entries[o + J_B]
	var c := entries[o + J_C]

	match verb:
		0:
			return "%s a %s" % [phrase, _kind(a)]
		1:
			return "%s a %s from the %s" % [phrase, _kind(a), _bloc(b)]
		2, 5, 7:
			return "%s building %d" % [phrase, int(a)]
		3:
			var lit := " with street lighting" if b > 0.5 else ""
			return "%s %s km of %s%s" % [
				phrase,
				Parts.clean(_span_km(entries, o)),
				_grade(a),
				lit,
			]
		4:
			return "%s %s km of %s" % [
				phrase, Parts.clean(_span_km(entries, o)), _utility(a),
			]
		6:
			var through := "through post %d" % int(b) if b > 0 else "off"
			var whose := "for building %d" % int(a) if a > 0 else "for the republic"
			return "%s %s %s" % [phrase, whose, through]
		8:
			return "%s building %d to keep %s t of %s" % [
				phrase, int(a), Parts.clean(c), _resource(b),
			]
		9:
			return "%s %d builders from the %s for building %d" % [
				phrase, int(c), _bloc(b), int(a),
			]
		10, 11:
			return "%s" % phrase
		12:
			var terms: String = (
				"buy up to %s t of" % Parts.clean(entries[o + J_D]) if c > 0.5
				else "sell all"
			)
			return "%s: %s %s with the %s" % [phrase, terms, _resource(a), _bloc(b)]
		13:
			return "%s from position %d" % [phrase, int(entries[o + J_E]) + 1]
		14:
			return "%s from position %d to %d" % [
				phrase, int(entries[o + J_E]) + 1, int(entries[o + J_F]) + 1,
			]
		15:
			return "%s of %s from the %s" % [phrase, Parts.thousands(c), _bloc(a)]
		16:
			return "%s: %s to the %s" % [phrase, Parts.thousands(c), _bloc(a)]
		17:
			return "%s to %s hours" % [phrase, Parts.clean(b)]
		18:
			# `c` says which of the two `a` is, because a building id and a kind
			# index are both small numbers.
			var whose := (
				"at building %d" % int(a) if c > 0.5 else "for every %s" % _kind(a)
			)
			if b < 0.0:
				return "dropped the hours rule %s" % whose
			return "%s %s to %s" % [phrase, whose, Parts.clean(b)]
		19:
			if int(b) == 0:
				return "mothballed building %d" % int(a)
			return "%s building %d to %d crew%s" % [
				phrase, int(a), int(b), "" if int(b) == 1 else "s",
			]
		20:
			return "%s building %d %s" % [phrase, int(a), _standing(b)]
		21:
			# The one string a command carries, and it is the player's own words.
			var name: String = _republic.journal_text(index)
			return "%s %s" % [phrase, name] if name != "" else phrase
		_:
			return phrase


## How long a road or a line was, from the two ends the order named.
##
## Straight-line, which is what the order was: a way is laid between two points
## and the simulation does not bend it.
func _span_km(entries: PackedFloat32Array, o: int) -> float:
	var from := Vector2(entries[o + J_C], entries[o + J_D])
	var to := Vector2(entries[o + J_E], entries[o + J_F])
	return from.distance_to(to) / 1000.0


func _kind(index: float) -> String:
	return String(_republic.building_kind_name(int(index)))


func _bloc(index: float) -> String:
	var i := int(index)
	return BLOC_WORDS[i] if i >= 0 and i < BLOC_WORDS.size() else "—"


func _resource(index: float) -> String:
	var names: PackedStringArray = _republic.resource_names()
	var i := int(index)
	return String(names[i]) if i >= 0 and i < names.size() else "—"


func _grade(index: float) -> String:
	var names: PackedStringArray = _republic.grade_names()
	var i := int(index)
	return String(names[i]).to_lower() if i >= 0 and i < names.size() else "way"


func _utility(index: float) -> String:
	var names: PackedStringArray = _republic.utility_names()
	var i := int(index)
	return String(names[i]).to_lower() if i >= 0 and i < names.size() else "line"


func _standing(index: float) -> String:
	var names: PackedStringArray = _republic.priority_names()
	var i := int(index)
	return String(names[i]).to_lower() if i >= 0 and i < names.size() else "—"


## Check the phrase table against the roster it shadows.
##
## Called by `--check`. This is the one table in the project that is a list of
## words indexed by a simulation enum with no other way to write it, and a table
## one row short prints a blank line where a command should be — which reads as a
## command that did nothing rather than as a bug.
func check(republic: Republic) -> String:
	if PHRASES.size() != republic.journal_verbs():
		return "the journal names %d verbs and the simulation has %d" % [
			PHRASES.size(), republic.journal_verbs(),
		]
	return ""
