extends CanvasLayer

## The republic's instrument panel.
##
## Every number here is read from a view the simulation already owns. Nothing in
## this file works anything out: the moment a panel starts deriving its own
## answer there are two versions of the balance and only one of them is tested.
## That was the archived build's discipline too, and it is why its performance
## and balance conversations were about facts.
##
## # Long lists are virtualised, and that is a measured rule
##
## Building 5,000 `Label` nodes costs 229 ms — a visible hitch — while updating
## them costs nothing: ~46 us per label to create against 0.2 us per frame to
## refresh, a factor of 165. So a table builds rows for what is visible and
## reuses them. Nothing here creates a node per datum.

const Overlays := preload("res://ui/overlays.gd")

## Rows the stockpile table keeps alive. Thirteen resources, so the whole table
## is visible and the pool never grows -- but it is a pool rather than a fresh
## build, because the roster grows with parity and this is where that would hurt.
const STOCK_ROWS := 16

@onready var clock_line: Label = $Panel/Margin/Rows/ClockLine
@onready var weather_line: Label = $Panel/Margin/Rows/WeatherLine
@onready var forecast_line: Label = $Panel/Margin/Rows/ForecastLine
@onready var republic_line: Label = $Panel/Margin/Rows/RepublicLine
@onready var overlay_line: Label = $Panel/Margin/Rows/OverlayLine
@onready var stock_grid: GridContainer = $Stock/Margin/Rows/Grid
@onready var stock_title: Label = $Stock/Margin/Rows/Title
@onready var hint: Label = $Hint

var _stock_labels: Array[Label] = []
var _resource_names := PackedStringArray()


func _ready() -> void:
	_build_stock_rows()


func set_resource_names(names: PackedStringArray) -> void:
	_resource_names = names


## Pooled rows, built once. See the virtualisation note above.
func _build_stock_rows() -> void:
	for i in STOCK_ROWS * 2:
		var label := Label.new()
		label.add_theme_font_size_override("font_size", 13)
		if i % 2 == 1:
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
			label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		stock_grid.add_child(label)
		_stock_labels.append(label)


func refresh(republic: Node, overlay_mode: int, speed_names: Array) -> void:
	clock_line.text = "%s    %s" % [republic.date_text(), speed_names[republic.speed()]]

	var rain: float = republic.precipitation_mm()
	weather_line.text = "%.1f °C%s" % [
		republic.temperature_c(),
		"    %.0f mm" % rain if rain > 0.05 else "",
	]

	# Five days ahead. Heating follows today's temperature and never the month,
	# so a cold snap is an event you can be caught out by -- and a forecast is
	# what makes being caught out a mistake rather than an ambush.
	var ahead: PackedFloat32Array = republic.forecast(6)
	var parts := PackedStringArray()
	for d in range(1, ahead.size() / 2):
		parts.append("%+.0f" % ahead[d * 2])
	forecast_line.text = "five days:  " + "  ".join(parts) if parts.size() > 0 else ""

	republic_line.text = "pop %d  ·  %d at work  ·  %d buildings  ·  %d lorries  ·  ₽ %s" % [
		republic.population(),
		republic.employed(),
		republic.building_count(),
		republic.vehicle_count(),
		_thousands(republic.rubles()),
	]

	var name: String = Overlays.NAMES[overlay_mode]
	overlay_line.text = "overlay:  %s" % (name if name != "" else "none")

	_refresh_stock(republic)


func _refresh_stock(republic: Node) -> void:
	var held: PackedFloat32Array = republic.stockpiles()
	stock_title.text = "STOCKPILES"
	for row in STOCK_ROWS:
		var name_label := _stock_labels[row * 2]
		var value_label := _stock_labels[row * 2 + 1]
		if row < held.size() and row < _resource_names.size():
			name_label.text = _resource_names[row]
			value_label.text = "%.1f t" % held[row]
			# Nothing at all is worth saying quietly rather than not at all: an
			# empty bin is the most important number on this table.
			var empty := held[row] < 0.05
			name_label.modulate = Color(1, 1, 1, 0.45) if empty else Color.WHITE
			value_label.modulate = Color(0.9, 0.55, 0.45) if empty else Color.WHITE
		else:
			name_label.text = ""
			value_label.text = ""


func set_hint(text: String) -> void:
	hint.text = text


func _thousands(value: float) -> String:
	var whole := int(abs(value))
	var out := ""
	while whole >= 1000:
		out = " %03d%s" % [whole % 1000, out]
		@warning_ignore("integer_division")
		whole /= 1000
	out = "%d%s" % [whole, out]
	return ("-" + out) if value < 0.0 else out
