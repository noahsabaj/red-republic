extends RefCounted

## The chrome every screen wears, built in one place.
##
## **This is what makes "one consistent theme" a thing the code enforces rather
## than a thing a comment claims.** Before it, nine screens each set their own
## margins, their own title size and their own footer, and they agreed by
## coincidence -- so a tenth screen would have agreed by coincidence too, until
## it did not. A screen now asks for a sheet and gets the title block, the rule
## under it, the notice line and the footer, in the same places, at the same
## sizes, every time.
##
## # The device: every screen is issued by an organ of the state
##
## The right-hand side of the title block names where the paper came from -- the
## Ministry of Labour, the Central Archive. It is the one piece of decoration in
## the whole interface, and it earns its place by being the thing that makes nine
## unrelated screens read as one government.
##
## The two screens that are the *player's* rather than the republic's -- settings
## and the pause overlay -- carry the build instead. That is not an inconsistency
## to tidy away: they genuinely are not issued by anybody in the fiction, and
## giving them an invented department would be the one lie on the screen.
##
## Deliberately no `class_name`, for the reason `looks.gd` gives. Callers
## `preload`.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")


## Build a full-screen sheet into `host`.
##
## Returns `{"body": VBoxContainer, "footer": HBoxContainer, "notice": Label,
## "title": Label, "organ": Label}` -- the body is where a screen's content goes,
## the footer is where its buttons go, and the notice is the one line every
## screen has for saying why something was refused.
##
## `wash` is how opaque the backdrop is. **Anything less than 1.0 shows whatever
## is behind it, and 0.97 is not close enough** -- the paper is dark and the text
## over it is not, so three per cent reads as a ghost of another screen printed
## underneath. It was caught at 0.86, "fixed" at 0.97, and a rendered frame
## showed it still happening. A full-screen sheet takes 1.0; a number below that
## belongs to an overlay that is meant to be seen through.
static func build(
	host: CanvasLayer,
	title: String,
	organ: String,
	blurb: String = "",
	wash: float = 1.0
) -> Dictionary:
	host.add_child(backdrop(wash))

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", P.MARGIN_SCREEN)
	margin.add_theme_constant_override("margin_right", P.MARGIN_SCREEN)
	margin.add_theme_constant_override("margin_top", P.MARGIN_SCREEN_TOP)
	margin.add_theme_constant_override("margin_bottom", P.GAP_SECTION)
	host.add_child(margin)

	var sheet := VBoxContainer.new()
	sheet.add_theme_constant_override("separation", P.GAP_WIDE)
	margin.add_child(sheet)

	var header := PanelContainer.new()
	header.theme_type_variation = "Header"
	sheet.add_child(header)

	var block := VBoxContainer.new()
	block.add_theme_constant_override("separation", P.GAP_TIGHT)
	header.add_child(block)

	var line := HBoxContainer.new()
	line.alignment = BoxContainer.ALIGNMENT_CENTER
	block.add_child(line)

	var title_label := Parts.say(title.to_upper(), "Title")
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(title_label)

	var organ_label := Parts.field(organ)
	organ_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	line.add_child(organ_label)

	var blurb_label := Parts.prose(blurb, "Body")
	blurb_label.visible = blurb != ""
	block.add_child(blurb_label)

	# **One notice line per screen, always present and usually empty.** Every
	# refusal in this game is a sentence the simulation wrote, and a screen that
	# had nowhere to put one would either invent a dialog or swallow it. Reserving
	# the space also stops the layout jumping when a refusal arrives.
	var notice := Parts.say("", "Alarm")
	notice.custom_minimum_size = Vector2(0, 18)
	sheet.add_child(notice)

	var body := VBoxContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation", P.GAP_WIDE)
	sheet.add_child(body)

	sheet.add_child(Parts.rule())

	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", P.GAP)
	sheet.add_child(footer)

	return {
		"body": body,
		"footer": footer,
		"notice": notice,
		"title": title_label,
		"organ": organ_label,
	}


## The wash behind a screen. Swallows clicks, which is what stops a press landing
## on the map behind a menu.
static func backdrop(alpha: float = 1.0) -> ColorRect:
	var rect := ColorRect.new()
	rect.color = Color(P.INK.r, P.INK.g, P.INK.b, alpha)
	rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	rect.mouse_filter = Control.MOUSE_FILTER_STOP
	return rect


## Put a screen's closing controls in the footer: a Back on the left, and
## whatever the screen's own action is on the right.
##
## Every screen closes the same way and from the same corner, which is worth a
## helper of its own -- a Back button that moves between screens is a screen a
## player has to re-read.
static func close_button(footer: HBoxContainer, text: String, on_press: Callable) -> Button:
	var back := Parts.button(text, "Quiet")
	back.pressed.connect(on_press)
	footer.add_child(back)
	footer.add_child(Parts.fill())
	return back
