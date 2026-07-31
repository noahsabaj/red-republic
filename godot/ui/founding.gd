extends CanvasLayer

## Founding: choosing your land, in two beats.
##
## Beat one is the shelf -- six candidate postings from one master seed, each a
## minimap and the few facts that decide a start. Beat two is the posting: a name,
## and a briefing specific to the land chosen.
##
## # The filters transform the shelf; they do not replace it
##
## Changing size or climate keeps the same candidate seeds and re-derives them, so
## flipping to taiga shows what that does to land the player was already
## considering. That is the design decision the whole screen exists to hold, and
## it is `Shelf::refilter` that implements it -- this file only has to not undo it
## by resetting the selection.
##
## # It blocks, so it says so
##
## A shelf costs 219 ms at 6 km, 611 ms at 10 km and 1,630 ms at 16 km, measured.
## The figure recorded in the design was the smallest map's attached to the
## default's, which is exactly how a screen ends up with no loading state. There
## is one, at every size.
##
## # Nothing here scores a posting
##
## `promise` is the simulation's own weighting and it orders and tints the cards.
## Every underlying figure is on the card so the player can disagree with it. A
## shell that computed its own idea of good land would be a second copy of a
## balance decision, and land quality genuinely varies -- a bad hand is a
## legitimate hand and papering over one would make the shelf dishonest.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

## Floats per card, matching `shelf::CARD_STRIDE` in the shell.
##
## Checked rather than trusted: `_read_cards` asks the shelf how many candidates
## it dealt and requires exactly that many rows of this width, so a stride change
## in Rust fails visibly here instead of silently shifting every column by one.
const CARD_STRIDE := 13

## How big a candidate's minimap is drawn.
##
## Sized against the screen rather than chosen: six of these plus the briefing
## under them has to fit in 900 px, and at 168 the cards pushed the whole footer
## -- including the button the screen exists for -- off the bottom of a rendered
## frame, with nothing but the frame to say so.
const MINIMAP_PX := 118

signal posting_taken
signal cancelled

var _republic: Republic = null
var _cards: Array = []
var _selected := -1
var _master_seed := 0
var _size_index := 1
var _climate_index := 0

var _card_row: HBoxContainer = null
var _card_panels: Array[PanelContainer] = []
var _briefing: RichTextLabel = null
var _name_box: LineEdit = null
var _notice: Label = null
var _take_button: Button = null
var _busy: Label = null
var _size_choice: OptionButton = null
var _climate_choice: OptionButton = null
var _seed_line: Label = null
var _terms: Label = null


func _ready() -> void:
	layer = 10
	var sheet: Dictionary = Sheet.build(
		self,
		"The Posting",
		"Council of Ministers",
		"Six sites have been surveyed. Choose one, and name it."
	)
	_notice = sheet["notice"]
	var body: VBoxContainer = sheet["body"]
	# Tighter than the sheet's rhythm, and this is the one screen that earns the
	# exception: six cards, a briefing and a name field is the most content any
	# screen in the game has to fit in one window, and the ordinary spacing put the
	# button the screen exists for half off the bottom of a rendered frame.
	body.add_theme_constant_override("separation", P.GAP)

	_build_filters(body)
	body.add_child(Parts.rule())

	_busy = Parts.say("", "Stamp")
	body.add_child(_busy)

	_card_row = HBoxContainer.new()
	_card_row.add_theme_constant_override("separation", P.GAP)
	_card_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(_card_row)

	_build_posting(body)

	Sheet.close_button(sheet["footer"], "BACK", func(): cancelled.emit())
	_take_button = Parts.button("TAKE THIS POSTING", "Primary")
	_take_button.pressed.connect(_take)
	sheet["footer"].add_child(_take_button)


func _build_filters(into: VBoxContainer) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", P.GAP_WIDE)
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	into.add_child(row)

	row.add_child(Parts.field("size"))
	_size_choice = OptionButton.new()
	_size_choice.item_selected.connect(_on_size_chosen)
	row.add_child(_size_choice)

	row.add_child(Parts.gap(P.GAP))
	row.add_child(Parts.field("climate"))
	_climate_choice = OptionButton.new()
	_climate_choice.item_selected.connect(_on_climate_chosen)
	row.add_child(_climate_choice)

	row.add_child(Parts.fill())
	# A fresh master seed: six new sites under the same filters. The filters
	# transform a shelf, and this is the one control that deliberately deals a new
	# hand -- so it is worded as going somewhere else rather than as a reroll.
	var another := Parts.button("SURVEY ELSEWHERE")
	another.pressed.connect(func(): _regenerate(true))
	row.add_child(another)


func _build_posting(into: VBoxContainer) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", P.GAP_SECTION * 2)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	into.add_child(row)

	var left := VBoxContainer.new()
	left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left.size_flags_stretch_ratio = 1.8
	left.add_theme_constant_override("separation", P.GAP_TIGHT)
	left.add_child(Parts.field("the briefing"))
	left.add_child(Parts.rule())
	_briefing = RichTextLabel.new()
	_briefing.bbcode_enabled = true
	# **It scrolls rather than fitting its content**, because a briefing that grew
	# to fit pushed the row of cards, the rule and the whole footer down until
	# "Take this posting" was off the bottom of the window. A panel that can push
	# the screen's own button out of reach is worse than one that scrolls.
	_briefing.fit_content = false
	_briefing.scroll_active = true
	_briefing.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left.add_child(_briefing)
	row.add_child(left)

	var right := VBoxContainer.new()
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right.add_theme_constant_override("separation", P.GAP_TIGHT)
	right.add_child(Parts.field("name of the republic"))
	right.add_child(Parts.rule())

	_name_box = LineEdit.new()
	_name_box.placeholder_text = "Krasnogorsk"
	_name_box.custom_minimum_size = Vector2(300, 40)
	_name_box.text_changed.connect(func(_t): _notice.text = "")
	_name_box.text_submitted.connect(func(_t): _take())
	right.add_child(_name_box)

	_seed_line = Parts.say("", "Faint")
	right.add_child(_seed_line)

	# **The terms sit beside the button that accepts them**, not at the end of the
	# briefing. They were its last paragraph, which put the single most
	# consequential sentence on the screen -- that a posting comes with money and
	# nothing else -- below the fold, while the column next to "Take this posting"
	# was empty from the seed line down.
	right.add_child(Parts.gap(P.GAP_WIDE))
	right.add_child(Parts.field("what Moscow sends"))
	right.add_child(Parts.rule())
	_terms = Parts.prose("", "Note")
	right.add_child(_terms)
	row.add_child(right)


## Open the screen. `master_seed` seeds the whole shelf.
func open(republic: Republic, master_seed: int) -> void:
	_republic = republic
	_master_seed = master_seed
	# The limit comes from the simulation so the box cannot accept a name the
	# command will refuse. Typing 48 here would be the shell holding its own copy
	# of a simulation constant, which is exactly how the founding once ended up
	# with more jobs than settlers.
	_name_box.max_length = _republic.name_limit()
	_fill_choices()
	_regenerate(false)


func _fill_choices() -> void:
	# Both rosters are read from the simulation. A list of climate names typed here
	# is a second copy of `ClimateId::ALL` and would silently stop matching the day
	# a fifth climate is authored.
	_size_choice.clear()
	for name in _republic.size_names():
		_size_choice.add_item(name)
	_size_choice.selected = _size_index

	_climate_choice.clear()
	for name in _republic.climate_names():
		_climate_choice.add_item(name)
	_climate_choice.selected = _climate_index


func _on_size_chosen(index: int) -> void:
	_size_index = index
	_regenerate(false)


func _on_climate_chosen(index: int) -> void:
	_climate_index = index
	_regenerate(false)


## Re-derive the shelf, showing the loading state while it blocks.
##
## The `await` is what makes the loading label appear at all: `derive_shelf`
## blocks the main thread for up to 1.6 seconds, so without yielding a frame first
## the label is set and the frame that would have drawn it never happens. The
## player sees a frozen window and then the cards, which reads as a hang.
func _regenerate(new_master: bool) -> void:
	if _republic == null:
		return
	if new_master:
		# A different master seed, derived from the current one rather than from the
		# clock, so the sequence a player walks through is reproducible from the one
		# number they started with.
		_master_seed = (_master_seed * 6364136223846793005 + 1442695040888963407) & 0x7FFFFFFF

	_busy.visible = true
	_busy.text = "SURVEYING %s GROUND…" % String(
		_republic.size_names()[_size_index]
	).to_upper()
	_set_interactive(false)
	await get_tree().process_frame

	if _cards.is_empty():
		_republic.derive_shelf(_master_seed, _size_index, _climate_index)
	else:
		_republic.refilter_shelf(_size_index, _climate_index)
	_read_cards()
	_build_cards()

	_busy.visible = false
	_set_interactive(true)
	# Keep the selection across a filter change: candidate n is still candidate n,
	# and dropping the selection would undo the one thing the filter rule exists to
	# provide.
	_select(_selected if _selected >= 0 else 0)


func _set_interactive(on: bool) -> void:
	_size_choice.disabled = not on
	_climate_choice.disabled = not on
	_take_button.disabled = not on


func _read_cards() -> void:
	var flat: PackedFloat32Array = _republic.shelf_cards()
	_cards.clear()
	# **The shelf says how many candidates it dealt; the card array must agree.**
	# Sized from the roster rather than from the array's own length, because a
	# divisibility check alone passes on a compensating error -- thirteen floats
	# lost and one candidate lost divide just as evenly as nothing wrong at all,
	# and the screen would silently deal five postings where six were derived.
	var dealt: int = _republic.shelf_size()
	if flat.size() != dealt * CARD_STRIDE:
		# Rust and this file disagree about the layout, which would otherwise show
		# as every number on every card being the wrong one. Loud, because a card
		# quietly reporting iron as coal is worse than an empty screen.
		push_error(
			"shelf card mismatch: %d floats is not %d candidates of %d"
			% [flat.size(), dealt, CARD_STRIDE]
		)
		return
	for i in dealt:
		var o := i * CARD_STRIDE
		_cards.append({
			"index": int(flat[o]),
			"promise": flat[o + 1],
			"buildable": flat[o + 2],
			"water": flat[o + 3],
			"forest": flat[o + 4],
			"coal": flat[o + 5],
			"iron": flat[o + 6],
			"oil": flat[o + 7],
			"groundwater": flat[o + 8],
			"coal_reach": flat[o + 9],
			"crossings_east": int(flat[o + 10]),
			"crossings_west": int(flat[o + 11]),
			"coldest": flat[o + 12],
		})


func _build_cards() -> void:
	for panel in _card_panels:
		panel.queue_free()
	_card_panels.clear()

	for card in _cards:
		# A `PanelContainer`, not a `Panel`: a Panel reports no minimum size from
		# anchored children, so the row of cards once came back zero pixels tall
		# and the posting row below was drawn straight over the top. Every node
		# existed and every label had its text -- only the rendered frame showed it.
		var panel := PanelContainer.new()
		panel.theme_type_variation = "Card"
		panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		panel.gui_input.connect(_on_card_input.bind(int(card["index"])))
		_card_row.add_child(panel)
		_card_panels.append(panel)

		var column := VBoxContainer.new()
		column.add_theme_constant_override("separation", P.GAP_TIGHT)
		panel.add_child(column)

		var picture := TextureRect.new()
		picture.texture = _minimap_texture(int(card["index"]))
		picture.custom_minimum_size = Vector2(MINIMAP_PX, MINIMAP_PX)
		picture.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		picture.stretch_mode = TextureRect.STRETCH_SCALE
		column.add_child(picture)

		# The score, tinted by itself. For sorting and colour only -- the figures
		# under it are what the player actually decides on.
		var promise: float = card["promise"]
		var score := Parts.say("%d%%" % int(round(promise * 100.0)), "FigureBig")
		score.add_theme_color_override("font_color", _promise_tone(promise))
		column.add_child(score)
		column.add_child(Parts.rule())

		# The facts pack tight against each other rather than taking the column's
		# rhythm. Nine of them at the card's ordinary spacing was eighty pixels of
		# air, which came straight out of the briefing under it -- and the briefing
		# is the half of this screen a player actually reads.
		var facts := VBoxContainer.new()
		facts.add_theme_constant_override("separation", 1)
		column.add_child(facts)

		for line in _card_lines(card):
			var fact := HBoxContainer.new()
			fact.add_theme_constant_override("separation", P.GAP_TIGHT)
			var name_label := Parts.say(String(line[0]), "Faint")
			name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			fact.add_child(name_label)
			var value := Parts.figure(String(line[1]), "Figure")
			value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
			value.add_theme_color_override("font_color", line[2])
			fact.add_child(value)
			facts.add_child(fact)


## The facts on a card, each with the tone it should read in.
##
## Deliberately few, and coal reach is deliberately first among the geology: it is
## the single most decisive fact about a posting. Coal under your feet is a
## republic; coal twelve kilometres away is a haulage problem.
func _card_lines(card: Dictionary) -> Array:
	var reach: float = card["coal_reach"]
	var reach_text := "none"
	var reach_tone := P.ALARM
	if reach >= 0.0:
		reach_text = "%.1f km" % (reach / 1000.0)
		# Beyond about four kilometres the coal is a road-building project before it
		# is a mine, which is worth flagging on the card rather than discovering
		# afterwards.
		reach_tone = P.PAPER if reach < 4000.0 else P.ALARM

	var west: int = card["crossings_west"]
	return [
		["coal at", reach_text, reach_tone],
		["buildable", "%d%%" % int(round(float(card["buildable"]) * 100.0)), P.PAPER_DIM],
		["coal", Parts.bulk(card["coal"]), P.PAPER_DIM],
		["iron", Parts.bulk(card["iron"]), P.PAPER_DIM],
		["oil", Parts.bulk(card["oil"]), P.PAPER_DIM],
		["water", "%d%%" % int(round(float(card["water"]) * 100.0)), P.PAPER_DIM],
		["forest", "%d%%" % int(round(float(card["forest"]) * 100.0)), P.PAPER_DIM],
		# East and West are not interchangeable and this is the line that says so: a
		# customs house clears only for the bloc whose post it stands at, so a
		# posting with no Western post earns no dollars until one is reachable.
		["posts", "%dE %dW" % [card["crossings_east"], west],
			P.PAPER_DIM if west > 0 else P.ALARM],
		["coldest", "%+.0f °C" % card["coldest"], P.PAPER_DIM],
	]


func _promise_tone(promise: float) -> Color:
	if promise >= 0.62:
		return P.GOOD
	if promise >= 0.42:
		return P.PAPER
	return P.ALARM


## A candidate's land as a texture, rasterised from its terrain.
func _minimap_texture(index: int) -> ImageTexture:
	var raw: PackedByteArray = _republic.shelf_minimap(index, MINIMAP_PX)
	var expected := MINIMAP_PX * MINIMAP_PX * 3
	if raw.size() != expected:
		return null
	var image := Image.create_from_data(
		MINIMAP_PX, MINIMAP_PX, false, Image.FORMAT_RGB8, raw
	)
	return ImageTexture.create_from_image(image)


func _on_card_input(event: InputEvent, index: int) -> void:
	if event is InputEventMouseButton and event.pressed \
			and event.button_index == MOUSE_BUTTON_LEFT:
		_select(index)


func _select(index: int) -> void:
	if _cards.is_empty():
		return
	_selected = clampi(index, 0, _cards.size() - 1)
	for i in _card_panels.size():
		# The chosen card is the one with the red rule round it. A theme variation
		# rather than a stylebox built here: what "chosen" looks like is the theme's
		# to say, and this file's job is only to say which one it is.
		_card_panels[i].theme_type_variation = "CardChosen" if i == _selected else "Card"
	_write_briefing()
	_seed_line.text = "site %d  ·  seed %d" % [
		_selected + 1, _republic.candidate_seed(_selected)
	]


## The briefing, specific to the land chosen.
##
## Written from the candidate's own figures and its climate's authored year, so it
## says something a player could not read off the card: what the winter does, when
## the growing season is, and which bloc this posting can actually trade with.
func _write_briefing() -> void:
	if _selected < 0 or _selected >= _cards.size():
		_briefing.text = ""
		return
	var card: Dictionary = _cards[_selected]
	var year: PackedFloat32Array = _republic.climate_year(_climate_index)
	var climate := String(_republic.climate_names()[_climate_index])
	var extent_km := int(round(_republic.size_extents()[_size_index] / 1000.0))

	var lines: Array[String] = []
	lines.append(
		"[b]%s[/b], %d km across. %s" % [climate, extent_km, _land_sentence(card)]
	)

	if year.size() >= 24:
		var coldest := 99.0
		var coldest_month := 0
		var growing := 0
		for m in 12:
			if year[m] < coldest:
				coldest = year[m]
				coldest_month = m
			# The farm gates on frost and ramps with temperature; nothing in it reads
			# the month. Five degrees is a fair proxy for a month that grows
			# anything, which is all a briefing needs to say.
			if year[m] >= 5.0:
				growing += 1
		var rain := 0.0
		for m in 12:
			rain += year[12 + m] * 30.0
		lines.append(
			"%s is the hard month at %+.0f °C. %d months warm enough to grow, %.0f mm of rain a year."
			% [MONTHS[coldest_month], coldest, growing, rain]
		)

	var west: int = card["crossings_west"]
	if west == 0:
		lines.append(
			"[b]No Western post.[/b] A customs house clears only for the bloc whose "
			+ "post it stands at, so this posting earns roubles and no dollars."
		)
	else:
		lines.append(
			"%d Western and %d Eastern posts. Hard currency means hauling to a Western one."
			% [west, card["crossings_east"]]
		)

	_briefing.text = "\n\n".join(lines)

	# This used to promise settlers and a town. It does not any more, because a
	# posting no longer comes with one: the map is empty and the grant is the whole
	# of what Moscow sends. Product text is a claim, and that one stopped being true
	# the day the founding stopped placing buildings.
	_terms.text = (
		"%s roubles, and nothing else. No buildings, no people. Hire a Bloc firm to "
		% Parts.thousands(_republic.founding_grant())
		+ "raise the first of it, and hard currency is something you will have to earn."
	)


func _land_sentence(card: Dictionary) -> String:
	var reach: float = card["coal_reach"]
	if reach < 0.0:
		return "The survey found no workable coal at all."
	if reach < 1500.0:
		return "Coal is under the site itself."
	if reach < 4000.0:
		return "Coal is %.1f km out — a road before it is a mine." % (reach / 1000.0)
	return "Coal is %.1f km out. This is a haulage problem before it is a republic." % (
		reach / 1000.0
	)


const MONTHS := [
	"January", "February", "March", "April", "May", "June",
	"July", "August", "September", "October", "November", "December",
]


## Take the posting. The refusal is the simulation's own sentence.
func _take() -> void:
	if _republic == null or _selected < 0:
		return
	var why: String = _republic.take_posting(_selected, _name_box.text)
	if why != "":
		_notice.text = why
		_name_box.grab_focus()
		return
	_notice.text = ""
	posting_taken.emit()


func focus_name() -> void:
	if _name_box != null:
		_name_box.grab_focus()
