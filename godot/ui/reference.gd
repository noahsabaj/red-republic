extends CanvasLayer

## The reference: how the game explains itself.
##
## No advisor and no scripted tutorial. Everything below the prose is composed
## from the republic's own authored tables, so it cannot describe a republic that
## does not exist -- and a building added to the simulation documents itself with
## nobody doing anything, because this file walks `building_kind_count()` rather
## than a list of entries somebody wrote.
##
## # It used to be written in Rust, and that was the interface in the wrong crate
##
## `crates/red-republic-shell/src/reference.rs` composed these headings, these
## entry titles, these label-and-value lines and this prose, and shipped the
## result over as marked-up text for this file to style. So the wording, the order
## of the facts on a building, and the decision to say "nothing -- it must be
## imported" all lived where nobody could see them beside the screen they appear
## on. What crosses the boundary now is figures and names; every sentence in the
## reference is in this file.
##
## The claim that came with it survived the move and got stronger. The old
## module's test asserted that every authored row appeared somewhere in the text;
## that is no longer a thing that can fail, because the roster *is* the loop.
##
## # One node, not one node per row
##
## Each section is a single `RichTextLabel` of generated BBCode. That is the
## answer to the measured rule that building a `Label` costs about 165 times what
## updating one costs -- 5,000 of them is a visible 229 ms hitch, and a reference
## of several hundred entries would be exactly that hitch every time it opened.
## One label sidesteps the question rather than virtualising around it.
##
## Only the open section is composed, so opening the reference costs one section
## and not five.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal closed

## The sections, in the order a stranger needs them: how it works, then what the
## goods are, then what makes them, then what moves them, then what they move on.
const SECTIONS := ["How it works", "Goods", "Buildings", "Vehicles", "Ways & lines"]

## Which section of a building's goods each line of `building_flows` belongs to.
## Matches `tables::building_flows` in the shell.
const FLOW_CONSUMES := 0
const FLOW_PRODUCES := 1
const FLOW_MATERIALS := 2
const FLOW_SELLS := 3

var _republic: Republic = null
var _body: RichTextLabel = null
var _tabs: HBoxContainer = null
var _tab_buttons: Array[Button] = []
var _open := 0

## The rosters, read once per open. Every one is a list of names authored beside
## the thing it names, in the simulation -- a copy of any of them here would be
## the copy that stops matching.
var _resources := PackedStringArray()
var _forms := PackedStringArray()
var _media := PackedStringArray()
var _minerals := PackedStringArray()
var _schooling := PackedStringArray()
var _shapes := PackedStringArray()


func _ready() -> void:
	layer = 12
	var sheet: Dictionary = Sheet.build(
		self,
		"Reference",
		"State Publishing House",
		"Compiled from the republic's own tables. If it is in the game, it is here."
	)
	var body: VBoxContainer = sheet["body"]

	_tabs = HBoxContainer.new()
	_tabs.add_theme_constant_override("separation", 0)
	body.add_child(_tabs)
	for i in SECTIONS.size():
		var tab := Parts.button(SECTIONS[i].to_upper(), "Tab")
		tab.toggle_mode = true
		tab.pressed.connect(_show_section.bind(i))
		_tabs.add_child(tab)
		_tab_buttons.append(tab)
	_tabs.add_child(Parts.fill())

	_body = RichTextLabel.new()
	_body.bbcode_enabled = true
	_body.scroll_active = true
	_body.selection_enabled = true
	_body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.add_child(_body)

	Sheet.close_button(sheet["footer"], "BACK", func(): closed.emit())


## Open the reference. Re-read every time, which is what keeps it honest -- a
## cached document is a document that can be stale.
func open(republic: Republic) -> void:
	_republic = republic
	_resources = republic.resource_names()
	_forms = republic.resource_forms()
	_media = republic.way_names()
	_minerals = republic.mineral_names()
	_schooling = republic.schooling_names()
	_shapes = republic.form_names()
	_show_section(_open)


func _show_section(index: int) -> void:
	_open = clampi(index, 0, SECTIONS.size() - 1)
	for i in _tab_buttons.size():
		_tab_buttons[i].set_pressed_no_signal(i == _open)
	if _republic == null:
		return
	_body.text = _compose(_open)
	_body.scroll_to_line(0)


## Turn to one section, for a capture run. See `main.gd::_open_named_screen`.
func show_page(index: int) -> void:
	_show_section(index)


## One section's composed text, for the load check in `main.gd`.
##
## The claim this screen makes is that it is compiled from the rosters, so a
## building added to the simulation documents itself. Nothing in `cargo test` can
## see GDScript, so the claim is checked where the composition now lives -- and
## the check needs to be able to ask for a section it is not looking at.
func section_text(index: int) -> String:
	return _compose(clampi(index, 0, SECTIONS.size() - 1))


func _compose(index: int) -> String:
	match index:
		0: return _playing()
		1: return _goods()
		2: return _buildings()
		3: return _vehicles()
		_: return _ways()


# ---- the prose ---------------------------------------------------------------
#
# **About invariants, never about numbers.** Every rule below is something the
# design holds fixed, and a stranger cannot play without it. Not one of them
# states a quantity: quantities come from the tables, where they cannot drift out
# of step with the simulation. That boundary is the only reason hand-written text
# is allowed on a screen whose point is that it is compiled.

const PLAYING := [
	[
		"Time",
		"One real second is one in-game second at the first speed. The others buy an "
		+ "hour, two, four or eight of in-game time per second. Nothing is ever "
		+ "skipped — there is no way to make time pass without spending it.",
	],
	[
		"Nothing happens instantly",
		"Everything ordered is a site: a bill of materials, builder-days, and a place "
		+ "your lorries deliver to. There is no button that makes a site finish. "
		+ "Anything sourced inside the republic costs labour and materials and no "
		+ "money at all.",
	],
	[
		"Builders are people who travel",
		"A Construction Office employs builders who commute to it like any other "
		+ "workplace, and it runs buses out to its sites. A remote site is expensive "
		+ "in vehicles, roads and time rather than in money. An office cannot be "
		+ "pulled down with its crews out, and neither can a site with a gang on it — "
		+ "recall them first.",
	],
	[
		"A building runs the hours you staff it for",
		"Every workplace runs a number of crews, and each crew works a shift. One crew "
		+ "on a standard day is what a building's figures describe; more crews is "
		+ "proportionally more work out of it, and proportionally more people. An hour "
		+ "nobody is rostered for is an hour that produces nothing, so running the "
		+ "night is a thing you pay for in workers rather than a switch.",
	],
	[
		"A longer shift is paid for by the people working it",
		"Shift length is national policy, with exceptions by trade and by building. "
		+ "Lengthening a shift buys more of the day out of the crew you already have — "
		+ "and costs them health and loyalty in proportion, every day. Loyalty is what "
		+ "decides whether people stay.",
	],
	[
		"Freight is vehicles",
		"Garages own vehicles, and a depot's establishment is fixed: wanting more "
		+ "lorries means another depot and the people to crew it. A vehicle never "
		+ "accepts a job it cannot finish, so running dry is a refusal in the yard "
		+ "rather than a lorry stranded in a field.",
	],
	[
		"The dark is an obstacle you build against",
		"A shift that starts or ends in the dark needs a lit way to work or a seat on "
		+ "something. Street lighting is a variant of paved road rather than something "
		+ "you place: it takes materials and builder-days by the kilometre and it draws "
		+ "off the grid, so a lit street wants a transformer in reach and a republic "
		+ "short of power puts its lamps out first. A bus is the other answer, and it "
		+ "costs fuel instead.",
	],
	[
		"Roads are preferred, never required",
		"A vehicle costs the road route against the open-ground straight line and takes "
		+ "whichever is quicker, so a road is something it chooses because it is "
		+ "faster. Traffic packs the ground it crosses, and a corridor worn hard enough "
		+ "becomes a dirt track on the map that nobody ordered. Roads never bog; that "
		+ "is the whole argument for building one.",
	],
	[
		"Water has to be bridged",
		"A bridge is its own grade and the only one that may span water. Until somebody "
		+ "pays for one, a river divides the republic.",
	],
	[
		"Power and heat are networks",
		"A plant lights only what it is strung to and a boiler warms only what a main "
		+ "runs past. A consumer plugs into a transformer station, and the station "
		+ "plugs into the line — a pylon strung past a factory does not run it. Heat "
		+ "leaks far faster than power does per kilometre, which is why district "
		+ "heating is a town-scale thing and a remote camp wants its own boiler.",
	],
	[
		"The border has two sides",
		"The whole perimeter is frontier, divided into stretches held by the Western "
		+ "Alliance or the Eastern Bloc, with posts on your own ground. You do not "
		+ "build a crossing; you build road out to one. A customs house clears only for "
		+ "the bloc whose post it stands at — so earning dollars means hauling to a "
		+ "Western post, and if the only Western post is across the map, that is what a "
		+ "dollar costs.",
	],
	[
		"People arrive, and people leave",
		"Contentment is a breakdown rather than a score, and the weakest component is "
		+ "what to fix. A republic that serves its people attracts more; one that fails "
		+ "them loses them. Settlers arrive at a frontier post and have to be fetched by "
		+ "a coach — nobody appears inside a housing block. A group nobody comes for "
		+ "goes home.",
	],
	[
		"Work has to be reachable",
		"A job nobody can reach goes unfilled however many people are out of work, and "
		+ "so does a job nobody is qualified for. Roughly two kilometres on foot; "
		+ "further only with transport, and transport is bounded by journey time rather "
		+ "than distance — which is what makes a faster road genuinely extend reach.",
	],
	[
		"The land is the difficulty",
		"There are no difficulty levels. A taiga posting costs measurably more to run "
		+ "than a plains one, and choosing a hard posting is a choice made inside the "
		+ "fiction. Socialist republics are not a walk in the park.",
	],
]


func _playing() -> String:
	var out := ""
	for rule in PLAYING:
		out += _entry(String(rule[0]))
		out += _paragraph(String(rule[1]))
	return out


# ---- the compiled sections ---------------------------------------------------


func _goods() -> String:
	var out := _paragraph(
		"What shape a good comes in decides which store will take it. A tank holds "
		+ "liquids, a silo holds grain and cement, a bay holds heaps. This governs "
		+ "deliveries only — a coal mine does not refuse its own coal."
	)

	# Who makes what and who eats what, gathered in one sweep of the roster rather
	# than by asking per resource. **"Nothing makes this" is a real and useful
	# answer** — it means the good has to be imported — so both lines appear
	# whether or not they have anything in them.
	var made_by := {}
	var used_by := {}
	for kind in _republic.building_kind_count():
		var name := String(_republic.building_kind_name(kind))
		for line in _flows(kind):
			var resource: int = line[1]
			if line[0] == FLOW_PRODUCES:
				_note(made_by, resource, name)
			elif line[0] == FLOW_CONSUMES:
				_note(used_by, resource, name)

	for i in _resources.size():
		out += _entry(String(_resources[i]))
		var rows := [
			["Form", String(_forms[i]) if i < _forms.size() else "—"],
			["Made by", _list_or(made_by.get(i, []), "nothing — it must be imported")],
			["Used by", _list_or(used_by.get(i, []), "nothing")],
		]
		out += _facts(rows)
	return out


func _buildings() -> String:
	var out := _paragraph(
		"Everything a republic can put up. A figure is what one crew on a standard "
		+ "day does; more crews is proportionally more, and the labour screen is "
		+ "where that is decided."
	)
	var table: PackedFloat32Array = _republic.building_table()
	var stride: int = _republic.building_stride()

	for kind in _republic.building_kind_count():
		var o := kind * stride
		if o + stride > table.size():
			break
		out += _entry(String(_republic.building_kind_name(kind)))

		var rows := [["Footprint", "%.0f x %.0f m" % [table[o], table[o + 1]]]]
		var workers := int(table[o + 2])
		if workers > 0:
			rows.append(["Staff", "%d, %s" % [workers, _needs(int(table[o + 3]))]])
		if int(table[o + 4]) > 0:
			rows.append(["Houses", "%d people" % int(table[o + 4])])
		if int(table[o + 5]) > 0:
			rows.append(["Beds for visitors", str(int(table[o + 5]))])

		var flows := _flows(kind)
		var eats := _goods_line(flows, FLOW_CONSUMES)
		if eats != "":
			rows.append(["Consumes", "%s a day" % eats])
		var makes := _goods_line(flows, FLOW_PRODUCES)
		if makes != "":
			rows.append(["Produces", "%s a day" % makes])
		var sells := _goods_line(flows, FLOW_SELLS)
		if sells != "":
			rows.append(["Sells", sells])

		if table[o + 8] > 0.0:
			rows.append(["Power", "%.0f kW" % table[o + 8]])
		if table[o + 9] > 0.0:
			rows.append(["Generates", "%.0f kW" % table[o + 9]])
		if table[o + 10] > 0.0:
			rows.append(["Heat", "%.0f kW" % table[o + 10]])
		if table[o + 11] > 0.0:
			rows.append(["Boiler", "%.0f kW" % table[o + 11]])
		if table[o + 12] > 0.0:
			rows.append(["Holds", "%.0f t" % table[o + 12]])
		var admits: PackedInt32Array = _republic.building_admits(kind)
		if admits.size() > 0:
			var shapes := PackedStringArray()
			for form in admits:
				if form >= 0 and form < _shapes.size():
					shapes.append(String(_shapes[form]))
			rows.append(["Accepts", ", ".join(shapes)])
		if int(table[o + 13]) > 0:
			rows.append(["Carries to work", "%d people a day" % int(table[o + 13])])
		var taps := int(table[o + 14])
		if taps >= 0 and taps < _minerals.size():
			rows.append(["Works", "%s deposits" % String(_minerals[taps])])

		var fleet := _fleet_line(kind)
		if fleet != "":
			rows.append(["Establishment", fleet])

		var materials := _goods_line(flows, FLOW_MATERIALS)
		rows.append([
			"To build",
			"%s, %.0f builder-days" % [
				materials if materials != "" else "nothing", table[o + 6],
			],
		])
		rows.append(["Contracted out", "%s ₽" % Parts.thousands(table[o + 7])])
		out += _facts(rows)
	return out


func _vehicles() -> String:
	var out := _paragraph(
		"A vehicle belongs to the garage that was built for it, and it is only as "
		+ "useful as the drivers that garage can staff. A journey leg carries the "
		+ "way's own speed limit and not the vehicle's, so the figures below are what "
		+ "it is capable of rather than what it will do."
	)
	var names := _republic.vehicle_kind_names()
	var table: PackedFloat32Array = _republic.vehicle_table()
	var stride: int = _republic.vehicle_stride()

	# **The ranking is made here, from the column this screen already has.** The
	# raw going figure is a margin against how hard the ground is, which means
	# nothing on its own; what a player needs is where this vehicle sits against
	# the others, and that is a comparison rather than a fact about a vehicle.
	var hardest := 1.0
	for i in names.size():
		if i * stride + 7 < table.size():
			hardest = maxf(hardest, table[i * stride + 7])

	for i in names.size():
		var o := i * stride
		if o + stride > table.size():
			break
		out += _entry(String(names[i]))
		var medium := int(table[o])
		var rows := [
			["Rides", String(_media[medium]) if medium < _media.size() else "—"],
		]
		if table[o + 1] > 0.0:
			rows.append(["Carries", "%.0f t" % table[o + 1]])
		if int(table[o + 2]) > 0:
			rows.append(["Seats", str(int(table[o + 2]))])
		rows.append([
			"Speed", "%.0f km/h on road, %.0f km/h cross-country" % [table[o + 3], table[o + 4]]
		])
		# **Kilogrammes, because tonnes printed as nothing.** The simulation holds
		# fuel in tonnes and a lorry burns 0.0003 of one per kilometre, so the
		# reference this replaced rendered every vehicle's consumption as
		# "0.00 t/km" — a figure on the page that said nothing at all, for as long
		# as the screen has existed. The unit belongs to the reader, not to the
		# store.
		rows.append([
			"Fuel", "%.1f kg/km, %.0f kg tank" % [table[o + 5] * 1000.0, table[o + 6] * 1000.0]
		])
		rows.append(["Going", "%s (%.2f)" % [_going(table[o + 7] / hardest), table[o + 7]]])
		out += _facts(rows)
	return out


func _ways() -> String:
	var out := _paragraph(
		"A journey leg carries the way's own speed limit, not the vehicle's — a lorry "
		+ "does the limit of the track it is on whatever it is capable of. Which is "
		+ "what makes a grade a decision."
	)
	var names := _republic.grade_names()
	var facts: PackedFloat32Array = _republic.grade_facts()
	var stride := 4

	for i in names.size():
		var o := i * stride
		if o + stride > facts.size():
			break
		out += _entry(String(names[i]))
		var medium := int(facts[o + 2])
		var rows := [
			["Carries", String(_media[medium]) if medium < _media.size() else "—"],
			["Limit", "%.0f km/h" % facts[o]],
			[
				"Per kilometre",
				"%s, %.0f builder-days" % [
					_pairs(_republic.grade_materials(i)), facts[o + 1],
				],
			],
		]
		if facts[o + 3] > 0.5:
			rows.append([
				"With street lamps",
				"%s more per kilometre, drawn off the grid" % _pairs(_republic.lamp_materials()),
			])
		out += _facts(rows)

	# **The other half of "laid between two points", and it was missing.** The
	# lines are ordered like a grade, built by the same crew out of the same
	# queue, and carry nothing until they are finished — the same shape as
	# everything above — and the whole roster of them was absent from a document
	# whose claim is that if it is in the game it is here.
	out += _paragraph(
		"A line is ordered exactly as a way is, and until the crew finish it, it "
		+ "carries nothing. What makes them different from each other is reach — "
		+ "how far a building may stand from one and still be plugged into it — and "
		+ "loss, which is charged against the length of the whole network rather "
		+ "than the span. That is why a sprawling grid is worse than a compact one, "
		+ "and why a remote camp wants its own boiler rather than a pipe from town."
	)
	var lines := _republic.utility_names()
	var table: PackedFloat32Array = _republic.utility_table()
	var line_stride: int = _republic.utility_stride()
	for i in lines.size():
		var o := i * line_stride
		if o + line_stride > table.size():
			break
		out += _entry(String(lines[i]))
		var rows := [
			["Reach", "%.0f m from the line" % table[o + 1]],
			[
				"Per kilometre",
				"%s, %.0f builder-days" % [
					_pairs(_republic.utility_materials(i)), table[o],
				],
			],
		]
		# A belt carries goods and a wire carries something that is not tonnage,
		# so the two are described by different figures rather than by the same
		# figure with a zero in it.
		if table[o + 4] > 0.5:
			rows.append(["Carries", "%.0f t a day, however long it is" % table[o + 3]])
		else:
			rows.append(["Lost", "%.1f%% over each kilometre of network" % (table[o + 2] * 100.0)])
		out += _facts(rows)
	return out


# ---- reading the packed views ------------------------------------------------


## One building's goods, as `[section, resource, tonnes]` rows.
func _flows(kind: int) -> Array:
	var packed: PackedFloat32Array = _republic.building_flows(kind)
	var out := []
	var i := 0
	while i + 3 <= packed.size():
		out.append([int(packed[i]), int(packed[i + 1]), packed[i + 2]])
		i += 3
	return out


## The goods of one section of one building, as a sentence.
func _goods_line(flows: Array, section: int) -> String:
	var parts := PackedStringArray()
	for line in flows:
		if line[0] != section:
			continue
		# A shelf line is a list rather than a rate, so it carries no tonnage and
		# printing "0.0 t" beside it would be inventing one.
		if section == FLOW_SELLS:
			parts.append(_resource_name(line[1]))
		else:
			parts.append("%.1f t %s" % [line[2], _resource_name(line[1])])
	return ", ".join(parts)


## A garage's establishment as a sentence, or empty for a building that keeps
## nothing.
func _fleet_line(kind: int) -> String:
	var packed: PackedFloat32Array = _republic.building_fleet(kind)
	var names := _republic.vehicle_kind_names()
	var parts := PackedStringArray()
	var i := 0
	while i + 2 <= packed.size():
		var which := int(packed[i])
		if which >= 0 and which < names.size():
			parts.append("%d x %s" % [int(packed[i + 1]), String(names[which])])
		i += 2
	return ", ".join(parts)


## A `[resource, tonnes]` list as a sentence.
func _pairs(packed: PackedFloat32Array) -> String:
	var parts := PackedStringArray()
	var i := 0
	while i + 2 <= packed.size():
		parts.append("%.1f t %s" % [packed[i + 1], _resource_name(int(packed[i]))])
		i += 2
	return ", ".join(parts) if parts.size() > 0 else "nothing"


func _resource_name(index: int) -> String:
	return String(_resources[index]) if index >= 0 and index < _resources.size() else "—"


## What a job's schooling requirement means in words.
##
## Not a footnote: a job nobody is qualified for goes unfilled however many people
## are out of work, and this is why a refinery will not open.
func _needs(level: int) -> String:
	if level <= 0:
		return "no schooling needed"
	if level >= _schooling.size():
		return "schooled"
	return "%s or better" % String(_schooling[level]).to_lower()


## Where a vehicle sits against the best cross-country thing the republic has.
func _going(share: float) -> String:
	if share >= 0.95:
		return "the best in the republic"
	if share >= 0.7:
		return "good"
	if share >= 0.45:
		return "fair"
	return "poor — keep it on made ground"


func _note(into: Dictionary, key: int, name: String) -> void:
	if not into.has(key):
		into[key] = []
	if not into[key].has(name):
		into[key].append(name)


func _list_or(names: Array, empty: String) -> String:
	return ", ".join(PackedStringArray(names)) if names.size() > 0 else empty


# ---- what a heading, an entry and a fact look like ---------------------------
#
# The whole of the reference's styling, in four functions. BBCode rather than
# nodes for the measured reason in the note at the top of this file.


func _entry(title: String) -> String:
	return "\n[font_size=%d][color=#%s][b]%s[/b][/color][/font_size]\n" % [
		P.SIZE_BODY + 2, P.PAPER.to_html(false), title,
	]


func _paragraph(text: String) -> String:
	return "[color=#%s]%s[/color]\n" % [P.PAPER_DIM.to_html(false), text]


## How wide the label column of an entry is, in monospaced characters.
##
## Sized against the longest label any section emits ("With street lamps", 17)
## with a space after it. A label longer than this pushes its own value out and
## nothing else, which is the failure worth having.
const LABEL_COLUMN := 19

## A block of label-and-value lines, in two columns that line up down the whole
## section.
##
## **Monospaced labels padded to a fixed width, rather than a `[table]`.** A
## BBCode table sizes its columns to its own contents, so every entry got a
## different column width -- "Rides / Road" put its value at 90 px and "Contracted
## out / 20 400 ₽" put its at 150, and the eye had to find the second column again
## on every heading. `[cell expand=]` does not change that. A fixed count of
## monospaced characters does, exactly, and it is also what the rest of this
## interface already does with a figure: the columns line up because the glyphs
## are the same width, not because a container was asked nicely.
func _facts(rows: Array) -> String:
	var out := ""
	for row in rows:
		out += "[font=%s][font_size=%d][color=#%s]%s[/color][/font_size][/font]%s\n" % [
			P.FACE_FIGURE,
			P.SIZE_SMALL,
			P.PAPER_FAINT.to_html(false),
			String(row[0]).rpad(LABEL_COLUMN),
			"[color=#%s]%s[/color]" % [P.PAPER_DIM.to_html(false), String(row[1])],
		]
	return out
