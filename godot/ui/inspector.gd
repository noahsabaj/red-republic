extends CanvasLayer

## One building, in front of the player: what it is, what it is doing, and what
## you may tell it to do.
##
## **This is the interaction the game was missing.** Clicking a building is how a
## planned economy is actually run — it is where the roster, the yard, the bill of
## materials, the standing orders and the import policy all meet, and every one of
## those was already finished in the simulation and reachable from nowhere. The
## shell carried thirteen bindings hanging off this panel before this panel
## existed.
##
## # A panel over the map, not a screen instead of it
##
## Every other form in this game is a full sheet, because every other form is
## about the whole republic. This one is about a thing standing in a field, and a
## full-screen page would hide the thing it is about — you would lose the site you
## just clicked, its neighbours, the road that does or does not reach it. So it
## takes the HUD's shape rather than `sheet.gd`'s: an instrument panel across the
## bottom, with the republic still visible above it.
##
## # Rebuilt per selection, refreshed per frame
##
## Unlike the build and labour screens, the rows here are not poolable: a coal
## mine, a house and a Construction Office have almost nothing in common, so the
## body is built when a different building is selected and only its figures are
## written after that. A selection is a click, not a frame, so the 165x cost of
## building a `Label` over updating one is paid at human speed.
##
## # Nothing here decides anything
##
## Every control hands back the simulation's own refusal and prints it. In
## particular the demolish button is never greyed out by a rule this file
## invented: a site with a gang standing on it is refused *by the simulation*,
## with a sentence saying to recall them first, and that sentence is more use than
## a dead button.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")

signal closed

## The layout of `views::building_state`, named once. Reading `state[7]` at nine
## call sites is how a stride change becomes a panel quietly showing the wrong
## column, and this file reads eleven of them.
const KIND := 0
const AT_X := 1
const AT_Y := 2
const BUILT := 3
const PROGRESS := 4
const WORK_DONE := 5
const LABOUR := 6
const STAFF := 7
const POSTS := 8
const SHIFTS := 9
const HOURS := 10
const STANDING := 11
const POWERED := 12
const POWER_DRAW := 13
const HEATED := 14
const HEAT_WANTED := 15
const LIVING := 16
const HOUSES := 17
const BEDS := 18
const STORAGE := 19
const CONTRACTOR := 20
const TAPPED := 21
const STALLED := 22
const PROVISIONED := 23
const COMFORTED := 24
const DRINK := 25
const POWER_MADE := 26
const HEAT_MADE := 27
const STATE_STRIDE := 28

## Why a building is stopped, in the order `views::stall_index` reports. Checked
## against `stall_count()` on open, because a table of words beside a simulation
## roster is the copy that silently loses a row.
const STALL_WORDS := ["nobody is working here", "no current", "the bins are empty"]

## What the two blocs are called. The simulation names them
## (`Market::name`), but only ever inside a refusal — so a panel that has to
## label a column reads these.
const BLOC_WORDS := ["Eastern Bloc", "Western Alliance"]

var _republic: Republic = null
var _store: RefCounted = null

## Which building is up, or 0 for none. Zero rather than -1 because ids are
## one-based, so zero is already a number no building has.
var _id := 0
## Which building the body was built for, so a refresh knows when to rebuild.
var _shown := -1
## Set while the demolish button is armed, when the player asked to be asked.
var _armed := false

var _panel: PanelContainer = null
var _title: Label = null
var _status: Label = null
var _notice: Label = null
var _body: HBoxContainer = null
var _demolish: Button = null

## The labels a refresh writes, by name. Built by `_build_body` and cleared with
## it, so a name that stops being built stops being written rather than throwing.
var _figures := {}


func _ready() -> void:
	# Above the HUD, below the full-screen sheets: this panel coexists with the
	# instrument panels and is covered by anything modal.
	layer = 14
	visible = false
	_build_chrome()


func _build_chrome() -> void:
	_panel = PanelContainer.new()
	_panel.theme_type_variation = "Instrument"
	_panel.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	_panel.offset_left = 410.0
	_panel.offset_right = -312.0
	# Clear of the hint line, which is the one thing on the screen that must
	# never be covered while a building is in hand.
	_panel.offset_bottom = -44.0
	_panel.offset_top = -330.0
	_panel.grow_vertical = Control.GROW_DIRECTION_BEGIN
	add_child(_panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", P.GAP_TIGHT)
	_panel.add_child(column)

	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", P.GAP_WIDE)
	head.alignment = BoxContainer.ALIGNMENT_CENTER
	column.add_child(head)

	_title = Parts.say("", "Section")
	head.add_child(_title)
	_status = Parts.say("", "Stamp")
	head.add_child(_status)
	head.add_child(Parts.fill())

	# **Demolish sits in the head rather than at the far end of a footer**, and
	# that is deliberate: it is the one irreversible thing on this panel, so it
	# reads better as a marked control beside the building's name than as the
	# last button on a row a player is already sweeping across.
	_demolish = Parts.button("DEMOLISH", "Quiet")
	_demolish.pressed.connect(_on_demolish)
	head.add_child(_demolish)

	var close := Parts.button("CLOSE", "Quiet")
	close.pressed.connect(close_panel)
	head.add_child(close)

	column.add_child(Parts.rule())

	# The one line this panel has for a refusal, always present and usually
	# empty — the same reservation `sheet.gd` makes, so the layout does not jump
	# when the simulation says no.
	_notice = Parts.say("", "Alarm")
	_notice.custom_minimum_size = Vector2(0, 18)
	_notice.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_notice)

	_body = HBoxContainer.new()
	_body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", P.GAP_SECTION)
	_body.clip_contents = true
	column.add_child(_body)


# ---- opening and closing -----------------------------------------------------


## Show a building. `id` of 0 closes the panel, which is what clicking open
## ground means.
func open(republic: Republic, store: RefCounted, id: int) -> void:
	_republic = republic
	_store = store
	if id <= 0:
		close_panel()
		return
	if id != _id:
		_armed = false
		_notice.text = ""
	_id = id
	visible = true
	refresh()


func close_panel() -> void:
	if not visible:
		return
	visible = false
	_id = 0
	_armed = false
	closed.emit()


func is_open() -> bool:
	return visible and _id > 0


func selected() -> int:
	return _id


# ---- the refresh -------------------------------------------------------------


func refresh() -> void:
	if _republic == null or _id <= 0:
		return
	var state: PackedFloat32Array = _republic.building_state(_id)
	# An empty read is the simulation saying this building is gone — pulled down
	# by this very panel a moment ago, most often. Closing is the honest response;
	# leaving the last figures on screen would be showing a building that is not
	# there.
	if state.size() < STATE_STRIDE:
		close_panel()
		return

	if _shown != _id:
		_build_body(state)
		_shown = _id

	_write_head(state)
	_write_figures(state)


func _write_head(state: PackedFloat32Array) -> void:
	var kind := int(state[KIND])
	_title.text = String(_republic.building_kind_name(kind)).to_upper()

	if state[BUILT] < 0.5:
		var who := int(state[CONTRACTOR])
		_status.text = "SITE  ·  %d%%%s" % [
			int(round(state[PROGRESS] * 100.0)),
			"  ·  %s" % BLOC_WORDS[who].to_upper() if who >= 0 else "",
		]
		_status.add_theme_color_override("font_color", P.OCHRE)
		return

	var stall := int(state[STALLED])
	if stall >= 0 and stall < STALL_WORDS.size():
		_status.text = "STOPPED  ·  %s" % STALL_WORDS[stall].to_upper()
		_status.add_theme_color_override("font_color", P.ALARM)
	elif int(state[SHIFTS]) == 0 and state[POSTS] > 0.0:
		_status.text = "MOTHBALLED"
		_status.add_theme_color_override("font_color", P.ALARM)
	else:
		_status.text = "OPEN"
		_status.add_theme_color_override("font_color", P.PAPER_DIM)


# ---- the body ----------------------------------------------------------------


## Build the columns this particular building wants.
##
## A house, a coal mine and a Construction Office share almost nothing, so the
## body is assembled per building rather than built once with most of it hidden.
## A panel of empty sections would be telling the player that a house has a
## roster it is not showing them.
func _build_body(state: PackedFloat32Array) -> void:
	for child in _body.get_children():
		child.queue_free()
	_figures.clear()

	_column_the_place(state)
	if state[BUILT] < 0.5:
		_column_the_site()
	else:
		_column_the_yard(state)
	if _republic.keeps_to_order(_id):
		_column_standing_orders()
	if state[HOUSES] > 0.0:
		_column_the_people()
	if state[POSTS] > 0.0 and state[BUILT] >= 0.5:
		_column_the_roster(state)
	# **Asked, not guessed.** The first version of this line offered hiring
	# wherever a building had staff to spare, and a rendered frame showed a coal
	# power plant offering to hire five builders. `can_hire` is the same rule the
	# command uses, so the column appears exactly where the verb would work.
	if String(_republic.can_hire(_id, 0)) == "" or String(_republic.can_hire(_id, 1)) == "":
		_column_the_office()


## A titled column inside the panel. `section` in `parts.gd` is a full screen's
## shape; this is the same idea at instrument scale.
func _section(title: String, weight: float = 1.0) -> VBoxContainer:
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.size_flags_stretch_ratio = weight
	box.add_theme_constant_override("separation", 1)
	_body.add_child(box)
	box.add_child(Parts.say(title.to_upper(), "Stamp"))
	box.add_child(Parts.rule())
	return box


## One label-and-figure line, registered under `key` so a refresh can write it.
##
## The caption is registered too, under `key + " caption"`, and the row itself
## under `key + " row"`. Three entries rather than one because the yard's rows are
## named by what happens to be in the yard, which changes while the panel is
## open — and reaching for a sibling by child index is how a layout change
## silently starts writing the wrong label.
func _line(into: VBoxContainer, key: String, label: String) -> Label:
	var line := HBoxContainer.new()
	line.custom_minimum_size = Vector2(0, 19)
	line.add_theme_constant_override("separation", P.GAP)
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	into.add_child(line)

	var name_label := Parts.say(label, "Faint")
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(name_label)

	var value := Parts.figure("")
	value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
	value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	value.size_flags_stretch_ratio = 1.6
	line.add_child(value)
	_figures[key] = value
	_figures["%s caption" % key] = name_label
	_figures["%s row" % key] = line
	return value


func _column_the_place(state: PackedFloat32Array) -> void:
	var box := _section("The place", 1.1)
	if state[BUILT] < 0.5:
		_line(box, "progress", "built")
		_line(box, "work", "builder-days")
	if state[POWER_DRAW] > 0.0:
		_line(box, "power", "current")
	if state[HEAT_WANTED] > 0.0:
		_line(box, "heat", "heat")
	if state[POWER_MADE] > 0.0 or state[HEAT_MADE] > 0.0:
		_line(box, "makes", "generates")
	if state[HOUSES] > 0.0:
		_line(box, "residents", "living here")
	if state[BEDS] > 0.0:
		_line(box, "beds", "beds")
	if state[STORAGE] > 0.0:
		_line(box, "storage", "holds")
	if state[TAPPED] > 0.0:
		_line(box, "tapped", "working body")
	_line(box, "where", "standing at")

	# Where this site buys what the republic cannot make. On every building
	# rather than only on sites, because the instruction outlives the site: a
	# finished building keeps its policy, and a player who set one wants to see
	# it and be able to drop it.
	box.add_child(Parts.gap(P.GAP_TIGHT))
	var post_line := HBoxContainer.new()
	post_line.custom_minimum_size = Vector2(0, 22)
	post_line.add_theme_constant_override("separation", P.GAP)
	post_line.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_child(post_line)
	var caption := Parts.say("imports through", "Faint")
	caption.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	post_line.add_child(caption)
	var post_value := Parts.figure("")
	post_value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
	post_line.add_child(post_value)
	_figures["imports"] = post_value
	# Stepped through the posts rather than chosen from a dropdown, for the
	# reason the standing is: there are four crossings on a frontier, they are
	# numbered, and a popup per building is a popup nobody opens. Zero is
	# "import nothing", which is the republic's default — auto-import spends hard
	# currency, so it stays off until somebody names a post.
	post_line.add_child(Parts.stepper(func(delta: float): _on_set_import(int(delta))))

	var own := Parts.button("SAME AS THE REPUBLIC", "Quiet")
	own.tooltip_text = "Drop this site's own instruction and follow the republic's policy"
	own.pressed.connect(_on_clear_import)
	box.add_child(own)
	_figures["import_button"] = own


## A half-built thing raises exactly two questions: is anyone on it, and if not,
## why not. The bill and the crew are the two answers.
func _column_the_site() -> void:
	var box := _section("Still wanted", 1.6)
	_line(box, "crew", "builders on site")

	var bill: PackedFloat32Array = _republic.site_bill(_id)
	var names: PackedStringArray = _republic.resource_names()
	var stride := 3
	@warning_ignore("integer_division")
	var rows: int = bill.size() / stride
	for i in rows:
		var resource := int(bill[i * stride])
		var label := String(names[resource]) if resource < names.size() else "—"
		_line(box, "bill%d" % i, label)
	if rows == 0:
		box.add_child(Parts.prose("Nothing but labour.", "Faint"))

	box.add_child(Parts.fill())
	var recall := Parts.button("RECALL THE CREW", "Quiet")
	recall.tooltip_text = (
		"Down tools. They wait where they stand for their office to send a bus."
	)
	recall.pressed.connect(_on_recall)
	box.add_child(recall)


func _column_the_yard(_state: PackedFloat32Array) -> void:
	var box := _section("The yard", 1.4)
	# Sized from the roster, never from a literal: a yard can hold any good, and
	# a fixed row count is the copy of a roster that silently loses a row.
	for i in _republic.resource_names().size():
		_line(box, "stock%d" % i, "")
	var empty := Parts.say("nothing at all", "Faint")
	box.add_child(empty)
	_figures["yard empty"] = empty


func _column_standing_orders() -> void:
	var box := _section("Keep on hand", 1.5)
	box.add_child(Parts.prose(
		"What lorries fetch here whether or not anything nearby has asked for it.",
		"Faint"
	))
	var names: PackedStringArray = _republic.resource_names()
	for resource in _republic.orderable(_id):
		var line := HBoxContainer.new()
		line.custom_minimum_size = Vector2(0, 22)
		line.add_theme_constant_override("separation", P.GAP)
		line.alignment = BoxContainer.ALIGNMENT_CENTER
		box.add_child(line)

		var label := Parts.say(
			String(names[resource]) if resource < names.size() else "—", "Faint"
		)
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		line.add_child(label)

		var value := Parts.figure("")
		value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
		value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		line.add_child(value)
		_figures["order%d" % resource] = value

		# Ten tonnes a press. One would be forty presses to fill a terminal, and
		# the order is a bulk instruction rather than a fine adjustment.
		line.add_child(Parts.stepper(
			func(delta: float): _on_order(resource, delta), 10.0
		))


func _column_the_people() -> void:
	var box := _section("The people", 1.3)
	_line(box, "provisions", "provisions")
	_line(box, "comforts", "comforts")
	_line(box, "drink", "drink")
	box.add_child(Parts.gap(P.GAP_TIGHT))
	# The contentment breakdown for *this* block, which is a different question
	# from the republic's average on the HUD: an estate at the end of a bad road
	# can be the only unhappy place in a contented republic.
	for i in _republic.contentment_names().size():
		_line(box, "content%d" % i, String(_republic.contentment_names()[i]).to_lower())
	_line(box, "worst", "worst of it")


func _column_the_roster(_state: PackedFloat32Array) -> void:
	var box := _section("The roster", 1.2)
	_line(box, "manning", "posts filled")
	_line(box, "day", "the day")

	var crews := HBoxContainer.new()
	crews.custom_minimum_size = Vector2(0, 22)
	crews.alignment = BoxContainer.ALIGNMENT_CENTER
	crews.add_theme_constant_override("separation", P.GAP)
	box.add_child(crews)
	var crew_label := Parts.say("crews", "Faint")
	crew_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	crews.add_child(crew_label)
	var crew_value := Parts.figure("")
	crew_value.add_theme_font_size_override("font_size", P.SIZE_TINY + 1)
	crews.add_child(crew_value)
	_figures["crews"] = crew_value
	crews.add_child(Parts.stepper(func(delta: float): _on_shifts(int(delta))))

	# The standing, cycled rather than chosen from a list — the same control the
	# labour screen uses, for the same reason: three values, two clicks at worst,
	# and a dropdown on a panel is a popup nobody opens.
	var standing := Parts.button("", "Quiet")
	standing.tooltip_text = (
		"Who gets people first when the republic is short of hands"
	)
	standing.pressed.connect(_on_cycle_standing)
	box.add_child(standing)
	_figures["standing"] = standing


## Builders hired abroad, on this office's books.
##
## Only on a Construction Office, which is the only place hiring means anything —
## and the refusal says so, so this column appearing is the panel agreeing with
## the simulation rather than deciding for itself.
func _column_the_office() -> void:
	var box := _section("Builders", 1.2)
	_line(box, "spare", "spare")
	_line(box, "foreign", "hired abroad")
	_line(box, "terms", "each costs")
	box.add_child(Parts.gap(P.GAP_TIGHT))
	for bloc in BLOC_WORDS.size():
		# A bloc with no frontier post on this map has nowhere for its workers to
		# arrive, and the simulation says so rather than this file guessing. The
		# refusal is printed in place of the button, which is more use than a
		# dead control.
		var why := String(_republic.can_hire(_id, bloc))
		if why != "":
			box.add_child(Parts.prose(why, "Faint"))
			continue
		var hire := Parts.button("HIRE 5 · %s" % BLOC_WORDS[bloc].to_upper(), "Quiet")
		hire.tooltip_text = (
			"They arrive at that bloc's frontier post and need a bus to fetch them"
		)
		hire.pressed.connect(func(): _on_hire(bloc))
		box.add_child(hire)


# ---- writing the figures -----------------------------------------------------


func _write(key: String, text: String, tone: Color = P.PAPER) -> void:
	var label = _figures.get(key)
	if label == null:
		return
	label.text = text
	label.add_theme_color_override("font_color", tone)


func _write_figures(state: PackedFloat32Array) -> void:
	_write("progress", "%d%%" % int(round(state[PROGRESS] * 100.0)))
	_write("work", "%s of %s" % [
		Parts.clean(state[WORK_DONE]), Parts.clean(state[LABOUR]),
	])
	if state[POWER_DRAW] > 0.0:
		var fed := state[POWERED] > 0.5
		_write("power", "%.1f MW  ·  %s" % [
			state[POWER_DRAW], "fed" if fed else "dark",
		], P.PAPER if fed else P.ALARM)
	if state[HEAT_WANTED] > 0.0:
		var warm := state[HEATED] > 0.5
		_write("heat", "%s  ·  %s" % [
			Parts.clean(state[HEAT_WANTED]), "reached" if warm else "cold",
		], P.PAPER if warm else P.ALARM)
	if state[POWER_MADE] > 0.0 or state[HEAT_MADE] > 0.0:
		var made := PackedStringArray()
		if state[POWER_MADE] > 0.0:
			made.append("%.1f MW" % state[POWER_MADE])
		if state[HEAT_MADE] > 0.0:
			made.append("%s heat" % Parts.clean(state[HEAT_MADE]))
		# At full activity, which is what the table authors -- a plant with
		# nobody in it makes none of this, and the roster column beside it is
		# where that shows.
		_write("makes", "  ·  ".join(made))
	_write("residents", "%d of %d" % [int(state[LIVING]), int(state[HOUSES])])
	_write("beds", "%d" % int(state[BEDS]))
	_write("storage", "%s each" % Parts.bulk(state[STORAGE]))
	_write("tapped", "no. %d" % int(state[TAPPED]))
	_write("where", "%.0f, %.0f m" % [state[AT_X], state[AT_Y]])

	_write_imports()
	if state[BUILT] < 0.5:
		_write_site()
	else:
		_write_yard()
	_write_orders()
	if state[HOUSES] > 0.0:
		_write_people(state)
	if state[POSTS] > 0.0 and state[BUILT] >= 0.5:
		_write_roster(state)
	_write_office()
	_write_demolish()


func _write_imports() -> void:
	var post: int = _republic.site_import_post(_id)
	var own: bool = _republic.site_has_own_import_policy(_id)
	if post == 0:
		_write("imports", "nothing", P.PAPER_FAINT)
	else:
		_write("imports", "post %d%s" % [post, "" if own else "  (the republic's)"])
	var button = _figures.get("import_button")
	if button != null:
		# Greyed by the simulation's own answer rather than by a rule invented
		# here: a site with no instruction of its own has nothing to clear, and
		# `ClearImportPolicy` refuses exactly that.
		button.disabled = not own


func _write_site() -> void:
	var crew: int = _republic.site_crew(_id)
	_write("crew", "%d" % crew, P.PAPER if crew > 0 else P.ALARM)

	var bill: PackedFloat32Array = _republic.site_bill(_id)
	var stride := 3
	@warning_ignore("integer_division")
	var rows: int = bill.size() / stride
	for i in rows:
		var wanted := bill[i * stride + 1]
		var here := bill[i * stride + 2]
		var resource := int(bill[i * stride])
		# What was bought abroad on this site's account, which is the difference
		# between "nobody has delivered yet" and "the goods were bought and taken
		# somewhere else" — two completely different problems that look identical
		# without it.
		var bought: float = _republic.site_bought_abroad(_id, resource)
		_write("bill%d" % i, "%s of %s%s" % [
			Parts.clean(here), Parts.clean(wanted),
			"  ·  %s bought" % Parts.clean(bought) if bought > 0.05 else "",
		], P.PAPER if here + 0.05 >= wanted else P.ALARM)


func _write_yard() -> void:
	var held: PackedFloat32Array = _republic.building_stock(_id)
	var names: PackedStringArray = _republic.resource_names()
	var stride := 2
	@warning_ignore("integer_division")
	var lines: int = held.size() / stride
	for i in names.size():
		var row = _figures.get("stock%d row" % i)
		if row == null:
			continue
		row.visible = i < lines
		if i >= lines:
			continue
		# Written here rather than at build time, because which goods a yard
		# holds changes while the panel is open.
		var resource := int(held[i * stride])
		_figures["stock%d caption" % i].text = (
			String(names[resource]) if resource < names.size() else "—"
		)
		_write("stock%d" % i, "%.1f t" % held[i * stride + 1])
	var empty = _figures.get("yard empty")
	if empty != null:
		empty.visible = lines == 0


func _write_orders() -> void:
	var orders: PackedFloat32Array = _republic.standing_orders(_id)
	var stride := 3
	# Everything orderable reads "none" unless the sweep below finds an order for
	# it, because a row left at its last value would be an instruction the player
	# had already cancelled.
	for resource in _republic.orderable(_id):
		_write("order%d" % resource, "none", P.PAPER_FAINT)
	@warning_ignore("integer_division")
	var rows: int = orders.size() / stride
	for i in rows:
		var resource := int(orders[i * stride])
		var here := orders[i * stride + 1]
		var ordered := orders[i * stride + 2]
		if ordered <= 0.0:
			continue
		_write("order%d" % resource, "%s of %s t" % [
			Parts.clean(here), Parts.clean(ordered),
		], P.PAPER if here + 0.05 >= ordered else P.ALARM)


func _write_people(state: PackedFloat32Array) -> void:
	_write("provisions", "%d%%" % int(round(state[PROVISIONED] * 100.0)),
		P.ALARM if state[PROVISIONED] < 0.5 else P.PAPER)
	# Comforts and drink lift contentment rather than being ways to fail, so
	# neither is ever coloured for being low — a player reading down this column
	# must not be invited to go and fix a bonus.
	_write("comforts", "+%d%%" % int(round(state[COMFORTED] * 100.0)), P.PAPER_DIM)
	_write("drink", "+%d%%" % int(round(state[DRINK] * 100.0)), P.PAPER_DIM)

	var content: PackedFloat32Array = _republic.home_contentment(_id)
	var names: PackedStringArray = _republic.contentment_names()
	# Components, then overall, then the comfort lift, then the index of the
	# worst — counted off the roster rather than indexed by a number typed here.
	if content.size() != names.size() + 3:
		return
	for i in names.size():
		_write("content%d" % i, "%d%%" % int(round(content[i] * 100.0)),
			P.ALARM if content[i] < 0.5 else P.PAPER)
	var worst := int(content[names.size() + 2])
	# The simulation's own answer, because a weighted loss is balance and balance
	# does not belong in a panel.
	_write("worst", String(names[worst]).to_lower() if worst >= 0 else "nothing",
		P.ALARM if worst >= 0 else P.PAPER_FAINT)


func _write_roster(state: PackedFloat32Array) -> void:
	var manned := int(state[STAFF])
	var posts := int(state[POSTS])
	_write("manning", "%d of %d" % [manned, posts],
		P.PAPER if manned >= posts else P.ALARM)
	var shifts := int(state[SHIFTS])
	if shifts == 0:
		_write("day", "closed", P.ALARM)
	else:
		_write("day", "%s h open" % Parts.clean(minf(shifts * state[HOURS], 24.0)))
	_write("crews", "%d" % shifts)
	var standings: PackedStringArray = _republic.priority_names()
	var rank := clampi(int(state[STANDING]), 0, maxi(standings.size() - 1, 0))
	var button = _figures.get("standing")
	if button != null and standings.size() > 0:
		button.text = "STANDING: %s" % String(standings[rank]).to_upper()


func _write_office() -> void:
	var spare: int = _republic.office_spare(_id)
	var hired: int = _republic.office_hired(_id)
	_write("spare", "%d" % spare, P.PAPER if spare > 0 else P.PAPER_FAINT)
	_write("foreign", "%d" % hired)
	var terms: PackedFloat32Array = _republic.hiring_terms()
	if terms.size() >= 2:
		_write("terms", "%s now  ·  %s a day" % [
			Parts.thousands(terms[0]), Parts.clean(terms[1]),
		])


## The demolish button says what pressing it will do.
##
## Two states, and the second only exists because the player asked for it in
## settings: `Confirm before demolishing` shipped as a row of the settings table
## that nothing read, which is a control for a verb that did not exist. It reads
## it now, and arms the button in place rather than opening a dialog — a modal
## over a map to ask one question is the shape this interface does not have.
func _write_demolish() -> void:
	if _armed:
		_demolish.text = "PRESS AGAIN TO PULL IT DOWN"
		_demolish.theme_type_variation = "Primary"
	else:
		_demolish.text = "DEMOLISH"
		_demolish.theme_type_variation = "Quiet"


# ---- the controls ------------------------------------------------------------


func _say(why: String) -> void:
	_notice.text = why


func _confirms() -> bool:
	return _store != null and bool(_store.get_value("play/confirm_demolish"))


func _on_demolish() -> void:
	if _confirms() and not _armed:
		_armed = true
		_say("")
		_write_demolish()
		return
	_armed = false
	var why: String = _republic.demolish(_id)
	if why != "":
		_say(why)
		_write_demolish()
		return
	close_panel()


func _on_recall() -> void:
	_say(_republic.recall_crew(_id))
	refresh()


## Step this site's import post, wrapping through "nothing" at both ends.
##
## The posts are read off the frontier rather than counted here: a frontier has
## as many crossings as worldgen gave it, and a number typed in this file would be
## a second copy of that — the copy that offers a post which does not exist.
func _on_set_import(delta: int) -> void:
	var posts: PackedFloat32Array = _republic.crossings()
	@warning_ignore("integer_division")
	var count: int = posts.size() / 4
	var current: int = _republic.site_import_post(_id)
	# 0..count, where 0 is "import nothing" — so there are count + 1 positions.
	var next := (current + delta + count + 1) % (count + 1)
	_say(_republic.set_import_post(_id, next))
	refresh()


func _on_clear_import() -> void:
	_say(_republic.clear_import_post(_id))
	refresh()


func _on_order(resource: int, delta: float) -> void:
	var orders: PackedFloat32Array = _republic.standing_orders(_id)
	var stride := 3
	var current := 0.0
	@warning_ignore("integer_division")
	var rows: int = orders.size() / stride
	for i in rows:
		if int(orders[i * stride]) == resource:
			current = orders[i * stride + 2]
			break
	_say(_republic.set_standing_order(_id, resource, maxf(current + delta, 0.0)))
	refresh()


func _on_shifts(delta: int) -> void:
	var state: PackedFloat32Array = _republic.building_state(_id)
	if state.size() < STATE_STRIDE:
		return
	var limits: PackedFloat32Array = _republic.shift_limits()
	var most := int(limits[2]) if limits.size() > 2 else 3
	_say(_republic.set_shifts(_id, clampi(int(state[SHIFTS]) + delta, 0, most)))
	refresh()


func _on_cycle_standing() -> void:
	var state: PackedFloat32Array = _republic.building_state(_id)
	if state.size() < STATE_STRIDE:
		return
	var count: int = _republic.priority_names().size()
	_say(_republic.set_priority(_id, (int(state[STANDING]) + 1) % maxi(count, 1)))
	refresh()


## Five at a time, which is a gang rather than a person.
func _on_hire(bloc: int) -> void:
	_say(_republic.hire_foreign(bloc, _id, 5))
	refresh()


## Check the words this panel keeps against the rosters they name.
##
## Called by `--check`. Two tables here are lists of words beside a simulation
## roster — the stall reasons and the bloc names — and that is the shape this
## project has watched silently lose a row five times. `cargo test` cannot see
## GDScript, so the comparison happens where GDScript runs.
func check(republic: Republic) -> String:
	if STALL_WORDS.size() != republic.stall_count():
		return "the inspector names %d stall reasons and the simulation has %d" % [
			STALL_WORDS.size(), republic.stall_count(),
		]
	if BLOC_WORDS.size() != republic.purse().size():
		return "the inspector names %d blocs and the treasury holds %d purses" % [
			BLOC_WORDS.size(), republic.purse().size(),
		]
	return ""
