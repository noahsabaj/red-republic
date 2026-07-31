extends RefCounted

## One place the menus get their look from.
##
## The HUD predates this and sets font sizes inline, which was fine for one
## panel and is not fine for six screens: five copies of a colour is five things
## to edit and four of them get missed. Everything M12 adds reads from here.
##
## Presentation only. Nothing in this file may change how a republic turns out.
##
## Deliberately no `class_name`, for the reason `looks.gd` gives: a global class
## is only registered by an editor import pass, so a freshly added one is
## invisible to `godot --path ...` until somebody re-imports. Callers `preload`.

## The survey look, carried into the UI so a panel and the ground agree.
##
## Read from `looks.gd` where it overlaps rather than re-typed, because the look
## was chosen once and a second copy of a chosen colour is the copy that drifts.
const INK := Color(0.88, 0.90, 0.91)
const INK_DIM := Color(0.62, 0.66, 0.69)
const INK_FAINT := Color(0.45, 0.49, 0.52)
## Institutional red, for the one thing on a screen that is the decision.
const ACCENT := Color(0.72, 0.24, 0.20)
const ACCENT_HOT := Color(0.84, 0.32, 0.26)
## A refusal, and a warning. The same tone the HUD already uses for a failing
## component, so "something is wrong" looks the same everywhere.
const ALARM := Color(0.90, 0.55, 0.45)
const GOOD := Color(0.52, 0.72, 0.54)

const PAPER := Color(0.09, 0.11, 0.13)
const PAPER_RAISED := Color(0.13, 0.16, 0.19)
const PAPER_SUNK := Color(0.06, 0.08, 0.09)
const RULE := Color(0.24, 0.28, 0.32)

const SIZE_TITLE := 34
const SIZE_HEAD := 20
const SIZE_BODY := 15
const SIZE_SMALL := 13
const SIZE_TINY := 11


## A filled panel with a hairline rule round it.
static func panel(fill: Color = PAPER_RAISED, border: Color = RULE) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = fill
	box.border_color = border
	box.set_border_width_all(1)
	box.set_content_margin_all(14.0)
	# Square. Rounded corners read as a phone app; this is a state instrument.
	return box


## A card: a bordered box that grows to fit whatever is put in it.
##
## **Use this rather than a bare `Panel`.** A `Panel` does not report a minimum
## size from its children -- children positioned with anchors contribute nothing --
## so a container hands it zero height and the *next* sibling is laid out on top of
## the card's contents. That is not a subtle failure and it is completely invisible
## to anything that is not a rendered frame: every node exists, every label has its
## text, and the screenshot shows two panels occupying the same pixels.
##
## It happened on the founding screen and the pause overlay in the same commit,
## which is why this is a helper instead of two fixes. `PanelContainer` sizes to
## its content, and the margin is inside it so callers add children directly.
static func card(fill: Color = PAPER_RAISED, border: Color = RULE, pad: int = 14) -> PanelContainer:
	var holder := PanelContainer.new()
	holder.add_theme_stylebox_override("panel", panel(fill, border))
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", pad)
	margin.add_theme_constant_override("margin_right", pad)
	margin.add_theme_constant_override("margin_top", pad)
	margin.add_theme_constant_override("margin_bottom", pad)
	holder.add_child(margin)
	return holder


## Where a card's children go.
##
## `card` wraps a margin inside the panel, so adding to the `PanelContainer`
## directly would put a second child beside the margin rather than inside it.
static func card_body(holder: PanelContainer) -> MarginContainer:
	return holder.get_child(0) as MarginContainer


static func heading(text: String, size: int = SIZE_HEAD, tone: Color = INK) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_color_override("font_color", tone)
	return label


static func body(text: String, tone: Color = INK_DIM) -> Label:
	return heading(text, SIZE_BODY, tone)


static func small(text: String, tone: Color = INK_DIM) -> Label:
	return heading(text, SIZE_SMALL, tone)


## A sentence or two of explanation, which wraps.
##
## **A `Label` does not wrap by default, so its minimum width is the whole
## sentence on one line.** Put one inside a column and that column can never be
## narrower than the text, whatever stretch ratio it was given. The build screen
## ran on that for as long as it has existed: BUILDINGS asked for twice the
## share and got half, because the one-line blurb under WAYS was setting the
## width of the panel, and every card in that column carried three hundred
## pixels of dead air between its name and its button.
##
## Any run of prose goes through this. [`small`] stays for short labels, where
## wrapping would be wrong and the minimum width is not a problem.
static func paragraph(text: String, tone: Color = INK_DIM) -> Label:
	var label := heading(text, SIZE_SMALL, tone)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	return label


## A rule across the container it is added to.
static func divider() -> Panel:
	var line := Panel.new()
	line.custom_minimum_size = Vector2(0, 1)
	var box := StyleBoxFlat.new()
	box.bg_color = RULE
	line.add_theme_stylebox_override("panel", box)
	return line


## A button in the institutional register: square, lettered, no icon.
##
## `primary` marks the one action a screen exists for. At most one per screen --
## two primary buttons is a screen that has not decided what it is for.
static func button(text: String, primary: bool = false) -> Button:
	var b := Button.new()
	b.text = text
	b.add_theme_font_size_override("font_size", SIZE_BODY)
	b.custom_minimum_size = Vector2(0, 38)
	# No emoji and no icons anywhere in product UI, which is a standing rule
	# rather than a style preference here.
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		var box := StyleBoxFlat.new()
		box.set_border_width_all(1)
		box.set_content_margin_all(10.0)
		match state:
			"normal":
				box.bg_color = ACCENT if primary else PAPER_RAISED
				box.border_color = ACCENT if primary else RULE
			"hover":
				box.bg_color = ACCENT_HOT if primary else Color(0.18, 0.21, 0.25)
				box.border_color = ACCENT_HOT if primary else INK_FAINT
			"pressed":
				box.bg_color = ACCENT if primary else PAPER_SUNK
				box.border_color = ACCENT_HOT if primary else INK_FAINT
			"disabled":
				box.bg_color = PAPER_SUNK
				box.border_color = Color(0.18, 0.20, 0.22)
			"focus":
				box.bg_color = Color(0, 0, 0, 0)
				box.border_color = INK_DIM
		b.add_theme_stylebox_override(state, box)
	b.add_theme_color_override("font_color", INK)
	b.add_theme_color_override("font_hover_color", Color.WHITE)
	b.add_theme_color_override("font_disabled_color", INK_FAINT)
	return b


## A full-screen wash behind a screen's content.
##
## Opaque for the menu, and semi-transparent for the pause overlay so the
## republic stays visible behind it -- a player pausing to think should still be
## looking at the thing they are thinking about.
##
## **Anything less than fully opaque shows the HUD through it, and 0.97 is not
## close enough.** The paper is dark and the HUD's text is not, so even three
## per cent of it reads as a ghost of another screen printed underneath. The
## build menu was set to 0.97 after 0.86 was caught doing exactly this, and a
## rendered frame of the labour screen showed it still happening -- the first
## fix moved the number and not the problem. A full-screen modal takes 1.0.
## Numbers below that belong to overlays that are *meant* to be see-through.
static func backdrop(alpha: float = 1.0) -> ColorRect:
	var rect := ColorRect.new()
	rect.color = Color(PAPER.r, PAPER.g, PAPER.b, alpha)
	rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	# A backdrop that swallows clicks is the point for a modal screen: it is what
	# stops a click landing on the map behind a menu.
	rect.mouse_filter = Control.MOUSE_FILTER_STOP
	return rect
