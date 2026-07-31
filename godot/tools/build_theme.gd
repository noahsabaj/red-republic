extends SceneTree

## Builds `res://ui/theme.tres`, the one look every screen inherits.
##
## Run:
##
##     godot --headless --path godot --script res://tools/build_theme.gd
##
## The generated resource is committed, and the project's default theme setting
## points at it -- so the editor previews what the game ships, and a `.tscn`
## authored in the editor needs no styling of its own. This is the same deal as
## `tools/build_icon.py` and `tools/fetch_art.py`: a checked-in generator, a
## committed artifact, and neither run as part of a build.
##
## # Why generated rather than hand-authored
##
## A `Theme` is about thirty `StyleBoxFlat`es, and in `.tres` each is nine lines
## of `bg_color = Color(...)` with a generated id. Written by hand that is seven
## hundred lines nobody can read and nobody can adjust: changing "the hairline is
## one shade lighter" means finding twenty-six literals. Written here it is one
## constant in `ui/palette.gd`. The `.tres` is the artifact; this is the source.
##
## # What belongs here and what does not
##
## Here: anything true of *every* button, every table row, every heading. In a
## screen: only what is true of that screen. If a screen sets a colour, either
## this file is missing a variation or the screen is wrong -- with the one
## exception of tinting a control by what the simulation says, which reads its
## colours from `ui/palette.gd` and never types a triple.

const P := preload("res://ui/palette.gd")


func _init() -> void:
	var theme := Theme.new()

	var text := _face(P.FACE_TEXT)
	var text_bold := _face(P.FACE_TEXT_BOLD)
	var text_italic := _face(P.FACE_TEXT_ITALIC)
	var figure := _face(P.FACE_FIGURE)
	# Capitals want air between them or they set as a solid block. This is the
	# one typographic move the whole interface leans on, so it is a face of its
	# own rather than a per-label override.
	var narrow := _face(P.FACE_NARROW, 1)
	var narrow_bold := _face(P.FACE_NARROW_BOLD, 2)
	var narrow_title := _face(P.FACE_NARROW_BOLD, 5)

	theme.default_font = text
	theme.default_font_size = P.SIZE_BODY

	_labels(theme, text, text_italic, figure, narrow, narrow_bold, narrow_title)
	_buttons(theme, narrow, figure)
	_panels(theme)
	_inputs(theme, text, narrow)
	_bars(theme)
	_containers(theme)
	_rich(theme, text, text_bold, text_italic, figure)

	var why := ResourceSaver.save(theme, "res://ui/theme.tres")
	if why != OK:
		printerr("could not write res://ui/theme.tres: %d" % why)
		quit(1)
		return
	print("wrote res://ui/theme.tres")
	quit()


## A font, optionally letterspaced.
##
## `FontVariation` is how Godot spaces glyphs; the alternative is a per-label
## constant, which is the sort of thing that gets applied to eleven of twelve
## headings.
func _face(path: String, spacing: int = 0) -> Font:
	var file: FontFile = load(path)
	if spacing == 0:
		return file
	var varied := FontVariation.new()
	varied.base_font = file
	varied.spacing_glyph = spacing
	return varied


# ---- labels ------------------------------------------------------------------


func _labels(
	theme: Theme,
	text: Font,
	italic: Font,
	figure: Font,
	narrow: Font,
	narrow_bold: Font,
	narrow_title: Font
) -> void:
	# The plain `Label`: what a label is if nobody said otherwise.
	theme.set_font("font", "Label", text)
	theme.set_font_size("font_size", "Label", P.SIZE_BODY)
	theme.set_color("font_color", "Label", P.PAPER)
	theme.set_color("font_outline_color", "Label", P.INK)
	theme.set_constant("outline_size", "Label", 0)
	theme.set_constant("line_spacing", "Label", 4)

	# name -> [font, size, colour]. A table rather than eleven near-identical
	# blocks, because the point of a scale is that it is one decision.
	var kinds := {
		"Title": [narrow_title, P.SIZE_TITLE, P.PAPER],
		"Section": [narrow_bold, P.SIZE_SECTION, P.PAPER],
		"Field": [narrow, P.SIZE_LABEL, P.PAPER_FAINT],
		"Body": [text, P.SIZE_BODY, P.PAPER_DIM],
		"Small": [text, P.SIZE_SMALL, P.PAPER_DIM],
		"Faint": [text, P.SIZE_SMALL, P.PAPER_FAINT],
		"Note": [italic, P.SIZE_BODY, P.PAPER_FAINT],
		"Figure": [figure, P.SIZE_FIGURE, P.PAPER],
		"FigureBig": [figure, P.SIZE_FIGURE_BIG, P.PAPER],
		"Stamp": [narrow_bold, P.SIZE_SMALL, P.OCHRE],
		"Alarm": [text, P.SIZE_SMALL, P.ALARM],
		"Good": [text, P.SIZE_SMALL, P.GOOD],
	}
	for name in kinds:
		var spec: Array = kinds[name]
		theme.set_type_variation(name, "Label")
		theme.set_font("font", name, spec[0])
		theme.set_font_size("font_size", name, spec[1])
		theme.set_color("font_color", name, spec[2])
		theme.set_constant("line_spacing", name, 4)


# ---- buttons -----------------------------------------------------------------


func _buttons(theme: Theme, narrow: Font, figure: Font) -> void:
	theme.set_font("font", "Button", narrow)
	theme.set_font_size("font_size", "Button", P.SIZE_LABEL)
	theme.set_color("font_color", "Button", P.PAPER_DIM)
	theme.set_color("font_hover_color", "Button", P.PAPER)
	theme.set_color("font_pressed_color", "Button", P.PAPER)
	theme.set_color("font_focus_color", "Button", P.PAPER)
	theme.set_color("font_disabled_color", "Button", P.PAPER_FAINT)
	theme.set_constant("h_separation", "Button", P.GAP)
	theme.set_stylebox("normal", "Button", _box(P.CARBON_RAISED, P.RULE, 14, 8))
	theme.set_stylebox("hover", "Button", _box(P.CARBON_RAISED.lightened(0.06), P.RULE_STRONG, 14, 8))
	theme.set_stylebox("pressed", "Button", _box(P.CARBON_SUNK, P.RULE_STRONG, 14, 8))
	theme.set_stylebox("disabled", "Button", _box(P.CARBON_SUNK, P.RULE, 14, 8))
	theme.set_stylebox("focus", "Button", _outline(P.RULE_STRONG, 14, 8))

	# **The one action a screen exists for.** At most one per screen: two primary
	# buttons is a screen that has not decided what it is for.
	theme.set_type_variation("Primary", "Button")
	theme.set_color("font_color", "Primary", P.PAPER)
	theme.set_color("font_hover_color", "Primary", Color.WHITE)
	theme.set_color("font_pressed_color", "Primary", Color.WHITE)
	theme.set_color("font_disabled_color", "Primary", P.PAPER_FAINT)
	theme.set_stylebox("normal", "Primary", _box(P.RED, P.RED, 18, 10))
	theme.set_stylebox("hover", "Primary", _box(P.RED_HOT, P.RED_HOT, 18, 10))
	theme.set_stylebox("pressed", "Primary", _box(P.RED.darkened(0.2), P.RED_HOT, 18, 10))
	theme.set_stylebox("disabled", "Primary", _box(P.CARBON_SUNK, P.RULE, 18, 10))
	theme.set_stylebox("focus", "Primary", _outline(P.PAPER, 18, 10))

	# A control that is present and is not the point: Back, Cancel, a tab that is
	# not the open one. No box at all until the cursor is on it.
	theme.set_type_variation("Quiet", "Button")
	theme.set_color("font_color", "Quiet", P.PAPER_FAINT)
	theme.set_stylebox("normal", "Quiet", _box(Color.TRANSPARENT, Color.TRANSPARENT, 10, 6))
	theme.set_stylebox("hover", "Quiet", _box(P.CARBON_RAISED, P.RULE, 10, 6))
	theme.set_stylebox("pressed", "Quiet", _box(P.CARBON_SUNK, P.RULE, 10, 6))
	theme.set_stylebox("disabled", "Quiet", _box(Color.TRANSPARENT, Color.TRANSPARENT, 10, 6))
	theme.set_stylebox("focus", "Quiet", _outline(P.RULE_STRONG, 10, 6))

	# A tab: an index card in a file. Nothing but a red rule under the open one,
	# which is the only piece of chrome on this screen that says where you are.
	theme.set_type_variation("Tab", "Button")
	theme.set_color("font_color", "Tab", P.PAPER_FAINT)
	theme.set_color("font_hover_color", "Tab", P.PAPER)
	theme.set_color("font_pressed_color", "Tab", P.PAPER)
	theme.set_stylebox("normal", "Tab", _underline(Color.TRANSPARENT, P.RULE, 14, 9))
	theme.set_stylebox("hover", "Tab", _underline(Color.TRANSPARENT, P.RULE_STRONG, 14, 9))
	theme.set_stylebox("pressed", "Tab", _underline(P.CARBON_RAISED, P.RED, 14, 9))
	theme.set_stylebox("focus", "Tab", _outline(P.RULE_STRONG, 14, 9))

	# A `-`, a `+`, a reset. Square, monospaced, and the same width whatever is
	# in it -- three steppers in a row must not shuffle as their glyphs change.
	theme.set_type_variation("Step", "Button")
	theme.set_font("font", "Step", figure)
	theme.set_font_size("font_size", "Step", P.SIZE_FIGURE)
	theme.set_color("font_color", "Step", P.PAPER_DIM)
	theme.set_color("font_hover_color", "Step", P.PAPER)
	theme.set_stylebox("normal", "Step", _box(P.CARBON_RAISED, P.RULE, 8, 5))
	theme.set_stylebox("hover", "Step", _box(P.CARBON_RAISED.lightened(0.08), P.RULE_STRONG, 8, 5))
	theme.set_stylebox("pressed", "Step", _box(P.CARBON_SUNK, P.RULE_STRONG, 8, 5))
	theme.set_stylebox("disabled", "Step", _box(P.CARBON_SUNK, P.RULE, 8, 5))
	theme.set_stylebox("focus", "Step", _outline(P.RULE_STRONG, 8, 5))

	# A box you tick. **A form has boxes; it does not have switches**, which is
	# also why the settings screen uses one of these rather than Godot's
	# `CheckButton` -- an iOS-shaped toggle in a ministry is the one thing on the
	# screen that would look imported.
	theme.set_type_variation("Toggle", "Button")
	theme.set_color("font_color", "Toggle", P.PAPER_FAINT)
	theme.set_color("font_pressed_color", "Toggle", P.PAPER)
	theme.set_color("font_hover_color", "Toggle", P.PAPER)
	theme.set_stylebox("normal", "Toggle", _box(P.CARBON_SUNK, P.RULE, 12, 7))
	theme.set_stylebox("hover", "Toggle", _box(P.CARBON_RAISED, P.RULE_STRONG, 12, 7))
	theme.set_stylebox("pressed", "Toggle", _box(P.RED.darkened(0.45), P.RED, 12, 7))
	theme.set_stylebox("disabled", "Toggle", _box(P.CARBON_SUNK, P.RULE, 12, 7))
	theme.set_stylebox("focus", "Toggle", _outline(P.RULE_STRONG, 12, 7))


# ---- panels ------------------------------------------------------------------


func _panels(theme: Theme) -> void:
	# The sheet a section is printed on.
	theme.set_stylebox("panel", "PanelContainer", _box(P.CARBON, P.RULE, P.PAD, P.PAD))
	theme.set_stylebox("panel", "Panel", _box(P.CARBON, P.RULE, P.PAD, P.PAD))

	# A card: one thing, in a box, that a player is asked to read or to press.
	theme.set_type_variation("Card", "PanelContainer")
	theme.set_stylebox("panel", "Card", _box(P.CARBON_RAISED, P.RULE, 12, 10))

	# The one card a player has chosen out of a row of them. **Exactly the padding
	# `Card` has**, because a chosen card with a different border width is a card
	# whose contents sit a pixel higher than its neighbours' -- so selecting one
	# nudged every figure on it out of line with the same figure beside it.
	theme.set_type_variation("CardChosen", "PanelContainer")
	theme.set_stylebox("panel", "CardChosen", _box(P.CARBON_RAISED, P.RED, 12, 10))

	# **A table row, not a card.** Rows are ruled, not boxed: a list of forty
	# bordered rectangles is forty things, and a ruled table is one thing with
	# forty lines in it. This is the single largest change to how the build and
	# labour screens read.
	theme.set_type_variation("Row", "PanelContainer")
	theme.set_stylebox("panel", "Row", _underline(P.CARBON, P.RULE, 12, 7))
	theme.set_type_variation("RowAlt", "PanelContainer")
	theme.set_stylebox("panel", "RowAlt", _underline(P.CARBON_RAISED, P.RULE, 12, 7))
	# The row under the cursor, and the row that is selected.
	theme.set_type_variation("RowHot", "PanelContainer")
	theme.set_stylebox("panel", "RowHot", _underline(P.CARBON_RAISED.lightened(0.05), P.RULE_STRONG, 12, 7))
	theme.set_type_variation("RowChosen", "PanelContainer")
	theme.set_stylebox("panel", "RowChosen", _box(P.CARBON_RAISED, P.RED, 12, 7))

	# A well: something read out of the republic rather than typed into it.
	theme.set_type_variation("Well", "PanelContainer")
	theme.set_stylebox("panel", "Well", _box(P.CARBON_SUNK, P.RULE, 12, 10))

	# The title block at the top of every screen. No fill and a red rule under
	# it -- the one piece of chrome that is the same on all nine screens.
	theme.set_type_variation("Header", "PanelContainer")
	theme.set_stylebox("panel", "Header", _rule_under(P.RED, 2, 0, 10))

	# The HUD's panels, which sit over the world rather than over a backdrop.
	# Nearly opaque: a number you cannot read because a lorry drove behind it is
	# not a number.
	theme.set_type_variation("Instrument", "PanelContainer")
	var over_world := _box(Color(P.CARBON.r, P.CARBON.g, P.CARBON.b, 0.93), P.RULE, 12, 9)
	theme.set_stylebox("panel", "Instrument", over_world)


# ---- inputs ------------------------------------------------------------------


func _inputs(theme: Theme, text: Font, narrow: Font) -> void:
	theme.set_font("font", "LineEdit", text)
	theme.set_font_size("font_size", "LineEdit", P.SIZE_BODY)
	theme.set_color("font_color", "LineEdit", P.PAPER)
	theme.set_color("font_placeholder_color", "LineEdit", P.PAPER_FAINT)
	theme.set_color("font_selected_color", "LineEdit", P.INK)
	theme.set_color("selection_color", "LineEdit", P.OCHRE)
	theme.set_color("caret_color", "LineEdit", P.RED_HOT)
	# A field on a form is a ruled line you write on, so the rule under it is the
	# whole control and the box round it is nearly invisible.
	theme.set_stylebox("normal", "LineEdit", _underline(P.CARBON_SUNK, P.RULE_STRONG, 12, 8))
	theme.set_stylebox("focus", "LineEdit", _underline(P.CARBON_SUNK, P.RED, 12, 8))
	theme.set_stylebox("read_only", "LineEdit", _underline(P.CARBON_SUNK, P.RULE, 12, 8))

	theme.set_font("font", "OptionButton", narrow)
	theme.set_font_size("font_size", "OptionButton", P.SIZE_LABEL)
	theme.set_color("font_color", "OptionButton", P.PAPER)
	theme.set_color("font_hover_color", "OptionButton", Color.WHITE)
	theme.set_color("font_pressed_color", "OptionButton", P.PAPER)
	theme.set_color("font_focus_color", "OptionButton", P.PAPER)
	theme.set_color("font_disabled_color", "OptionButton", P.PAPER_FAINT)
	# So the little triangle is the same ink as the words beside it rather than
	# the engine's default grey.
	theme.set_constant("modulate_arrow", "OptionButton", 1)
	theme.set_constant("arrow_margin", "OptionButton", P.GAP)
	theme.set_stylebox("normal", "OptionButton", _underline(P.CARBON_SUNK, P.RULE_STRONG, 12, 8))
	theme.set_stylebox("hover", "OptionButton", _underline(P.CARBON_RAISED, P.PAPER_FAINT, 12, 8))
	theme.set_stylebox("pressed", "OptionButton", _underline(P.CARBON_SUNK, P.RED, 12, 8))
	theme.set_stylebox("disabled", "OptionButton", _underline(P.CARBON_SUNK, P.RULE, 12, 8))
	theme.set_stylebox("focus", "OptionButton", _outline(P.RULE_STRONG, 12, 8))

	theme.set_font("font", "PopupMenu", text)
	theme.set_font_size("font_size", "PopupMenu", P.SIZE_BODY)
	theme.set_color("font_color", "PopupMenu", P.PAPER_DIM)
	theme.set_color("font_hover_color", "PopupMenu", P.PAPER)
	theme.set_color("font_disabled_color", "PopupMenu", P.PAPER_FAINT)
	theme.set_color("font_separator_color", "PopupMenu", P.PAPER_FAINT)
	theme.set_constant("v_separation", "PopupMenu", 2)
	theme.set_constant("item_start_padding", "PopupMenu", P.GAP)
	theme.set_constant("item_end_padding", "PopupMenu", P.GAP)
	theme.set_stylebox("panel", "PopupMenu", _box(P.CARBON, P.RULE_STRONG, 4, 4))
	theme.set_stylebox("hover", "PopupMenu", _box(P.CARBON_RAISED, P.CARBON_RAISED, 4, 4))
	var rule := StyleBoxLine.new()
	rule.color = P.RULE
	rule.thickness = 1
	theme.set_stylebox("separator", "PopupMenu", rule)

	# A slider is a gauge on an instrument: a sunk track with a red bar in it.
	theme.set_stylebox("slider", "HSlider", _bar(P.CARBON_SUNK, 6))
	theme.set_stylebox("grabber_area", "HSlider", _bar(P.RED, 6))
	theme.set_stylebox("grabber_area_highlight", "HSlider", _bar(P.RED_HOT, 6))
	theme.set_icon("grabber", "HSlider", _mark(P.PAPER, 8, 20))
	theme.set_icon("grabber_highlight", "HSlider", _mark(Color.WHITE, 8, 20))
	theme.set_icon("grabber_disabled", "HSlider", _mark(P.PAPER_FAINT, 8, 20))
	theme.set_constant("center_grabber", "HSlider", 1)


# ---- bars and separators -----------------------------------------------------


func _bars(theme: Theme) -> void:
	# Slim, and part of the ruling rather than a control in its own right.
	for bar in ["VScrollBar", "HScrollBar"]:
		var across := 6
		theme.set_stylebox("scroll", bar, _bar(P.CARBON_SUNK, across))
		theme.set_stylebox("grabber", bar, _bar(P.RULE_STRONG, across))
		theme.set_stylebox("grabber_highlight", bar, _bar(P.PAPER_FAINT, across))
		theme.set_stylebox("grabber_pressed", bar, _bar(P.PAPER_DIM, across))

	var line := StyleBoxLine.new()
	line.color = P.RULE
	line.thickness = 1
	theme.set_stylebox("separator", "HSeparator", line)
	var upright := StyleBoxLine.new()
	upright.color = P.RULE
	upright.thickness = 1
	upright.vertical = true
	theme.set_stylebox("separator", "VSeparator", upright)
	theme.set_constant("separation", "HSeparator", P.GAP)
	theme.set_constant("separation", "VSeparator", P.GAP)

	theme.set_stylebox("background", "ProgressBar", _bar(P.CARBON_SUNK, 8))
	theme.set_stylebox("fill", "ProgressBar", _bar(P.RED, 8))
	theme.set_color("font_color", "ProgressBar", P.PAPER)

	# The scroll viewport itself has no box: the panel it sits in already has one,
	# and two nested borders is the look of a dialog inside a dialog.
	theme.set_stylebox("panel", "ScrollContainer", StyleBoxEmpty.new())


# ---- containers --------------------------------------------------------------


func _containers(theme: Theme) -> void:
	theme.set_constant("separation", "BoxContainer", P.GAP)
	theme.set_constant("separation", "HBoxContainer", P.GAP)
	theme.set_constant("separation", "VBoxContainer", P.GAP)
	theme.set_constant("h_separation", "GridContainer", P.GAP_WIDE)
	theme.set_constant("v_separation", "GridContainer", 3)
	theme.set_constant("h_separation", "FlowContainer", P.GAP)
	theme.set_constant("v_separation", "FlowContainer", P.GAP)


# ---- rich text ---------------------------------------------------------------


func _rich(theme: Theme, text: Font, bold: Font, italic: Font, figure: Font) -> void:
	theme.set_font("normal_font", "RichTextLabel", text)
	theme.set_font("bold_font", "RichTextLabel", bold)
	theme.set_font("italics_font", "RichTextLabel", italic)
	theme.set_font("mono_font", "RichTextLabel", figure)
	theme.set_font_size("normal_font_size", "RichTextLabel", P.SIZE_BODY)
	theme.set_font_size("bold_font_size", "RichTextLabel", P.SIZE_BODY)
	theme.set_font_size("italics_font_size", "RichTextLabel", P.SIZE_BODY)
	theme.set_font_size("mono_font_size", "RichTextLabel", P.SIZE_FIGURE)
	theme.set_color("default_color", "RichTextLabel", P.PAPER_DIM)
	theme.set_constant("line_separation", "RichTextLabel", 4)
	theme.set_stylebox("normal", "RichTextLabel", StyleBoxEmpty.new())
	theme.set_stylebox("focus", "RichTextLabel", StyleBoxEmpty.new())


# ---- the shapes everything above is made of ----------------------------------


## A filled box with a hairline round it. Square corners, always: a rounded
## corner is a phone application and this is a state instrument.
func _box(fill: Color, border: Color, pad_x: int, pad_y: int) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = fill
	box.border_color = border
	box.set_border_width_all(1)
	box.content_margin_left = pad_x
	box.content_margin_right = pad_x
	box.content_margin_top = pad_y
	box.content_margin_bottom = pad_y
	return box


## A box ruled along the bottom only -- a row in a table, or a field on a form.
func _underline(fill: Color, border: Color, pad_x: int, pad_y: int) -> StyleBoxFlat:
	var box := _box(fill, border, pad_x, pad_y)
	box.set_border_width_all(0)
	box.border_width_bottom = 1
	return box


## Nothing but a rule, of a given weight, with air under it.
func _rule_under(colour: Color, weight: int, pad_x: int, pad_y: int) -> StyleBoxFlat:
	var box := _box(Color.TRANSPARENT, colour, pad_x, pad_y)
	box.set_border_width_all(0)
	box.border_width_bottom = weight
	return box


## An outline and no fill: what focus looks like.
func _outline(colour: Color, pad_x: int, pad_y: int) -> StyleBoxFlat:
	return _box(Color.TRANSPARENT, colour, pad_x, pad_y)


## A track or a fill for a slider or a bar.
func _bar(colour: Color, across: int) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = colour
	box.content_margin_top = across / 2.0
	box.content_margin_bottom = across / 2.0
	box.content_margin_left = across / 2.0
	box.content_margin_right = across / 2.0
	return box


## A plain rectangle of one colour, for the few theme items Godot wants a texture
## for rather than a box. `GradientTexture2D` is used because it serialises to
## text: a `.tres` that referenced a generated `.png` would be a theme with a
## loose file beside it that nothing regenerates.
func _mark(colour: Color, wide: int, tall: int) -> Texture2D:
	var ramp := Gradient.new()
	ramp.set_color(0, colour)
	ramp.set_color(1, colour)
	var tex := GradientTexture2D.new()
	tex.gradient = ramp
	tex.width = wide
	tex.height = tall
	return tex
