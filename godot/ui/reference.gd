extends CanvasLayer

## The reference: how the game explains itself.
##
## No advisor and no scripted tutorial. The content is generated in Rust from the
## authored tables, so it cannot describe a republic that does not exist -- see
## `crates/red-republic-shell/src/reference.rs`. This file only decides what a
## heading looks like.
##
## # One node, not one node per row
##
## The whole document is a single `RichTextLabel` with generated BBCode. That is a
## deliberate answer to the measured rule that building a `Label` costs about 165
## times what updating one costs -- 5,000 of them is a visible 229 ms hitch. A
## reference of several hundred entries would be exactly that hitch every time it
## opened. One label sidesteps the question rather than virtualising around it.

const Style := preload("res://ui/theme.gd")

signal closed

var _body: RichTextLabel = null
var _sections: HBoxContainer = null
var _lines := PackedStringArray()


func _ready() -> void:
	layer = 12
	add_child(Style.backdrop(0.97))

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 64)
	margin.add_theme_constant_override("margin_right", 64)
	margin.add_theme_constant_override("margin_top", 32)
	margin.add_theme_constant_override("margin_bottom", 26)
	add_child(margin)

	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 10)
	margin.add_child(rows)

	rows.add_child(Style.heading("REFERENCE", Style.SIZE_TITLE))
	rows.add_child(Style.small(
		"Generated from the republic's own tables. If it is in the game, it is here.",
		Style.INK_DIM
	))

	_sections = HBoxContainer.new()
	_sections.add_theme_constant_override("separation", 6)
	rows.add_child(_sections)
	rows.add_child(Style.divider())

	_body = RichTextLabel.new()
	_body.bbcode_enabled = true
	_body.scroll_active = true
	_body.selection_enabled = true
	_body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_body.add_theme_font_size_override("normal_font_size", Style.SIZE_SMALL)
	_body.add_theme_font_size_override("bold_font_size", Style.SIZE_BODY)
	_body.add_theme_color_override("default_color", Style.INK_DIM)
	rows.add_child(_body)

	rows.add_child(Style.divider())
	var footer := HBoxContainer.new()
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer.add_child(spacer)
	var back := Style.button("Back")
	back.pressed.connect(func(): closed.emit())
	footer.add_child(back)
	rows.add_child(footer)


## Build the document. Cheap enough to redo on every open, and redoing it is what
## keeps it honest -- a cached document is a document that can be stale.
func open(republic: Node) -> void:
	_lines = republic.reference()
	_compose()


func _compose() -> void:
	var text := ""
	for child in _sections.get_children():
		child.queue_free()

	for raw in _lines:
		var marker: String = raw.substr(0, 1)
		var rest: String = raw.substr(1)
		match marker:
			"#":
				_add_jump(rest)
				text += "\n[font_size=%d][color=#%s][b]%s[/b][/color][/font_size]\n" % [
					Style.SIZE_HEAD, Style.ACCENT_HOT.to_html(false), rest.to_upper()
				]
			">":
				text += "\n[color=#%s][b]%s[/b][/color]\n" % [
					Style.INK.to_html(false), rest
				]
			":":
				var parts: PackedStringArray = rest.split("\t")
				var label: String = parts[0] if parts.size() > 0 else rest
				var value: String = parts[1] if parts.size() > 1 else ""
				text += "    [color=#%s]%s[/color]   %s\n" % [
					Style.INK_FAINT.to_html(false), label, value
				]
			"-":
				text += "    · %s\n" % rest
			_:
				text += "%s\n" % rest

	_body.text = text


func _add_jump(section: String) -> void:
	var button := Style.button(section)
	button.custom_minimum_size = Vector2(0, 28)
	button.add_theme_font_size_override("font_size", Style.SIZE_SMALL)
	button.pressed.connect(func(): _jump_to(section))
	_sections.add_child(button)


## Scroll to a section.
##
## Godot counts *visual* lines, which is not the count built up while composing:
## a wrapped paragraph is several visual lines and the composer sees one. So the
## heading is found by searching the rendered text rather than by trusting the
## tally, which would drift further the wider the window.
func _jump_to(section: String) -> void:
	var target := section.to_upper()
	var parsed: PackedStringArray = _body.get_parsed_text().split("\n")
	for i in parsed.size():
		if parsed[i].strip_edges() == target:
			_body.scroll_to_line(clampi(i, 0, maxi(_body.get_line_count() - 1, 0)))
			return
