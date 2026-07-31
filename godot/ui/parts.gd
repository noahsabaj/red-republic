extends RefCounted

## The pieces every screen is built out of.
##
## **Nothing here chooses a colour, a size or a border.** Those are in
## `ui/theme.tres`, which the project applies to every `Control` before any of
## this runs; a part is a *shape* -- a row, a column head, a stepper -- named
## once so that nine screens cannot each invent their own.
##
## That split is the whole point, and it is what the file this replaced got
## wrong: the old `theme.gd` built a `StyleBoxFlat` per button per state at
## runtime, so "a button is square with a hairline" lived in a function that had
## to be called, and any screen could quietly not call it. A theme resource
## cannot be not-called.
##
## The one thing a part may take is a **theme type variation** -- `"Field"`,
## `"Primary"` -- which is a name in the theme rather than a value. If a caller
## needs a colour it reads `ui/palette.gd`, and the only callers that legitimately
## do are the ones tinting a control by what the simulation says.
##
## Deliberately no `class_name`, for the reason `looks.gd` gives. Callers
## `preload`.

const P := preload("res://ui/palette.gd")


# ---- words -------------------------------------------------------------------


## A line of text in one of the theme's voices.
##
## The variation names are the type scale: `Title`, `Section`, `Field`, `Body`,
## `Small`, `Faint`, `Note`, `Figure`, `FigureBig`, `Stamp`, `Alarm`, `Good`.
static func say(text: String, voice: String = "Body") -> Label:
	var label := Label.new()
	label.text = text
	label.theme_type_variation = voice
	return label


## A run of prose, which wraps.
##
## **A `Label` does not wrap by default, so its minimum width is the whole
## sentence on one line.** Put one in a column and that column can never be
## narrower than the text, whatever stretch ratio it was given -- which is how
## the build screen's BUILDINGS column asked for twice the share and got half.
## Anything longer than a few words goes through here.
static func prose(text: String, voice: String = "Body") -> Label:
	var label := say(text, voice)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	return label


## A caption in capitals: what a column holds, or what a control is for.
##
## The capitals are typed here rather than left to the caller, so a screen cannot
## half-apply the convention. The theme letterspaces them.
static func field(text: String) -> Label:
	return say(text.to_upper(), "Field")


## A number. Monospaced and right-aligned, because a column of figures that is
## not both is a column that shuffles as it updates.
static func figure(text: String, voice: String = "Figure") -> Label:
	var label := say(text, voice)
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	return label


# ---- controls ----------------------------------------------------------------


static func button(text: String, kind: String = "") -> Button:
	var b := Button.new()
	b.text = text
	if kind != "":
		b.theme_type_variation = kind
	b.custom_minimum_size = Vector2(0, P.BUTTON_HEIGHT)
	return b


## A `-` and a `+`. `nudge` takes the signed change.
##
## Square and monospaced, so three of them in a row line up and none of them
## changes width when its glyph does.
static func stepper(nudge: Callable, step: float = 1.0) -> HBoxContainer:
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 2)
	box.alignment = BoxContainer.ALIGNMENT_END
	for pair in [["-", -step], ["+", step]]:
		var b := button(String(pair[0]), "Step")
		b.custom_minimum_size = Vector2(P.STEP_BUTTON, P.STEP_BUTTON)
		var by: float = pair[1]
		b.pressed.connect(func(): nudge.call(by))
		box.add_child(b)
	return box


## A box you tick. Two states, both spelled out -- see the note on the `Toggle`
## variation in `tools/build_theme.gd` for why this is not a switch.
##
## **The caller writes the label, and that is not laziness.** The obvious version
## connected `toggled` to a lambda that captured the button, which is a reference
## cycle: the button holds the signal connection, the connection holds the
## lambda, the lambda holds the button, and Godot reports the pair as a leaked
## `ObjectDB` instance at exit. A caller that is already handling `toggled` to
## write the value can set the word in the same handler for nothing.
static func toggle() -> Button:
	var b := button("NO", "Toggle")
	b.toggle_mode = true
	b.custom_minimum_size = Vector2(64, P.BUTTON_HEIGHT)
	return b


## What a ticked box says. One place, so a screen cannot spell it two ways.
static func toggle_word(on: bool) -> String:
	return "YES" if on else "NO"


# ---- structure ---------------------------------------------------------------


## A spacer that eats whatever room is left, so what follows it sits at the far
## end of its container.
static func fill() -> Control:
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return spacer


## A fixed gap. Prefer container separation; this is for the one place a section
## wants more air than the rhythm gives it.
static func gap(pixels: int) -> Control:
	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0, pixels)
	return spacer


static func rule() -> HSeparator:
	return HSeparator.new()


## A scrolling column. Returns the column; the scroller is its parent.
##
## Horizontal scrolling is off on every list in this game: a table that can slide
## sideways is a table whose right-hand column can be invisible with nothing
## saying so.
static func scroller(into: Control, separation: int = 0) -> VBoxContainer:
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	into.add_child(scroll)

	var column := VBoxContainer.new()
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.add_theme_constant_override("separation", separation)
	scroll.add_child(column)
	return column


## One line of a table. Returns the box its cells go in.
##
## **Ruled, not boxed.** A list of forty bordered rectangles reads as forty
## things; a ruled table reads as one thing with forty lines in it. `alt` tints
## every other line, which is what lets an eye track across a wide row.
static func row(into: Control, alt: bool = false) -> HBoxContainer:
	var panel := PanelContainer.new()
	panel.theme_type_variation = "RowAlt" if alt else "Row"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	into.add_child(panel)

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", P.GAP_WIDE)
	line.custom_minimum_size = Vector2(0, P.ROW_HEIGHT)
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	panel.add_child(line)
	return line


## A boxed card: one thing a player is asked to read or to press.
##
## Returns the column its contents go in. Use a `row` for anything that is one of
## many; a card is for something that stands alone.
static func card(into: Control) -> VBoxContainer:
	var panel := PanelContainer.new()
	panel.theme_type_variation = "Card"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	into.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", P.GAP_TIGHT)
	panel.add_child(column)
	return column


## Put a control in a column of a table, at a share of the width.
##
## Every row in a table must call this with the same weights as its head, or the
## columns do not line up -- which is a thing no test can see and every rendered
## frame can.
static func cell(control: Control, weight: float, align: int = HORIZONTAL_ALIGNMENT_LEFT) -> Control:
	control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	control.size_flags_stretch_ratio = weight
	if control is Label:
		(control as Label).horizontal_alignment = align
	return control


## The head of a table: the column names, at the weights the rows will use.
##
## `columns` is `[[name, weight, alignment], ...]`. Taking the weights here and
## in `cell` from the same array at the call site is what keeps a head and its
## rows in step.
static func head(into: Control, columns: Array) -> HBoxContainer:
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", P.GAP_WIDE)
	# The head sits inside the same left and right padding a row has, or the
	# column names are offset from the figures under them by the row's margin.
	var pad := MarginContainer.new()
	pad.add_theme_constant_override("margin_left", 12)
	pad.add_theme_constant_override("margin_right", 12)
	pad.add_theme_constant_override("margin_bottom", 4)
	pad.add_child(line)
	into.add_child(pad)

	for column in columns:
		var align: int = column[2] if column.size() > 2 else HORIZONTAL_ALIGNMENT_LEFT
		line.add_child(cell(field(String(column[0])), float(column[1]), align))
	into.add_child(rule())
	return line


## A titled column with a scrolling list in it: the shape both halves of the
## build screen and both halves of the labour screen take.
##
## Returns the list. `weight` is its share of the width beside its sibling.
static func section(
	into: Control, title: String, blurb: String, weight: float = 1.0
) -> VBoxContainer:
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.size_flags_stretch_ratio = weight
	box.add_theme_constant_override("separation", P.GAP_TIGHT)
	into.add_child(box)

	box.add_child(say(title.to_upper(), "Section"))
	if blurb != "":
		box.add_child(prose(blurb, "Faint"))
	box.add_child(gap(P.GAP_TIGHT))
	return box


## Two columns side by side, each scrolling on its own.
##
## Stacked, a list of sixteen fills the screen by itself and the half a player
## acts on sits below the fold with the right third of every row empty. That was
## found by looking at a rendered frame of the labour screen and at nothing else.
static func columns(into: Control) -> HBoxContainer:
	var box := HBoxContainer.new()
	box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_theme_constant_override("separation", P.GAP_SECTION * 2)
	box.clip_contents = true
	into.add_child(box)
	return box


# ---- numbers -----------------------------------------------------------------


## Group a whole number with spaces, so 2500000 reads as 2 500 000.
##
## Spaces rather than commas: this is a republic counting roubles, and the space
## is the convention nearly everywhere that is not the one country this game is
## deliberately not set in.
static func thousands(value: float) -> String:
	var digits := str(int(abs(round(value))))
	var out := ""
	for i in digits.length():
		if i > 0 and (digits.length() - i) % 3 == 0:
			out += " "
		out += digits[i]
	return ("-" + out) if value < 0.0 else out


## A tonnage, at a sensible magnitude.
static func bulk(tonnes: float) -> String:
	if tonnes >= 1_000_000.0:
		return "%.1f Mt" % (tonnes / 1_000_000.0)
	if tonnes >= 1_000.0:
		return "%.0f kt" % (tonnes / 1_000.0)
	return "%.0f t" % tonnes


## A figure that is usually whole, printed without a pointless decimal.
static func clean(value: float) -> String:
	return "%d" % int(round(value)) if is_equal_approx(value, round(value)) else "%.1f" % value
