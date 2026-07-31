extends CanvasLayer

## The year in weather: what the winter did, and what it will do again.
##
## **The HUD forecasts five days and that is deliberately all it forecasts.** A
## cold snap is a thing a republic can be caught out by, and a five-day window is
## what makes being caught out a mistake rather than an ambush. This screen does
## not widen that window by a single day.
##
## What it does instead is give the player the other half of the same question.
## Heating follows today's temperature and never the month, so "how much fuel
## does a winter here take" is not answerable from a five-day strip -- and it is
## the largest standing order a young republic ever places. The answer is in the
## record: the days already lived, and the whole of last year if there is one.
## That is memory rather than prophecy, and it is exactly what a fuel decision is
## made from in a country that has been cold before.
##
## # Why the line goes quiet ahead of today
##
## `temperature_on_day` is a pure function of seed and day, so this screen could
## draw all twelve months of a year nobody has lived yet and every figure would
## be correct. It deliberately does not. Weather that can be read off a chart a
## year ahead is not weather; it is a schedule, and the whole heating mechanic
## stops being a risk the moment it becomes one. The five days the HUD already
## gives are drawn faint and the line stops there.
##
## # Its shape is the calendar's, not this file's
##
## Twelve months of thirty days is a decision the simulation made, and `calendar`
## hands it over rather than being copied here -- a screen that laid out its own
## year would keep drawing twelve columns on the day the year stopped having
## twelve months in it.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

signal closed

## The layout of `Republic.calendar`.
const C_DAY_INDEX := 0
const C_DAY_OF_YEAR := 1
const C_DAYS_PER_YEAR := 2
const C_DAYS_PER_MONTH := 3
const C_DAYS_ELAPSED := 4

## How far ahead the record is allowed to see. The same five days the HUD gives,
## and not one more -- see the module note.
const FORECAST_DAYS := 5

## The months, in the order the calendar counts them. Wording, so it is here and
## not in Rust; the *number* of them comes from `calendar` and this list is
## indexed modulo its length, so a year of a different shape reads out of this
## list rather than past the end of it.
const MONTHS := [
	"Jan", "Feb", "Mar", "Apr", "May", "Jun",
	"Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
]

var _republic: Republic = null
var _notice: Label = null
var _chart: Chart = null
var _this_year: VBoxContainer = null
var _last_year: VBoxContainer = null
var _built := false


func _ready() -> void:
	layer = 13
	visible = false


func open(republic: Republic) -> void:
	_republic = republic
	if not _built:
		_build()
		_built = true
	refresh()
	visible = true


func close() -> void:
	visible = false
	closed.emit()


func _build() -> void:
	var sheet: Dictionary = Sheet.build(
		self,
		"The Year",
		"Hydrometeorological Service",
		"Heating follows the day's temperature and never the month, so what a "
		+ "winter costs is a thing you find out by having had one. This is the "
		+ "record: what the year has done so far, and what last year did."
	)
	_notice = sheet["notice"]
	var body: VBoxContainer = sheet["body"]

	_chart = Chart.new()
	_chart.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_chart.custom_minimum_size = Vector2(0, 300)
	body.add_child(_chart)

	var columns := Parts.columns(body)
	_this_year = Parts.section(
		columns,
		"This year so far",
		"Counted from the first of January, over the days actually lived.",
		1.0
	)
	_last_year = Parts.section(
		columns,
		"Last year",
		"A whole year of it, which is the only complete winter there is to plan "
		+ "the next one against.",
		1.0
	)

	Sheet.close_button(sheet["footer"], "BACK", close)


## Read the record and lay it out.
##
## Two passes over at most two years of days, on open rather than per frame: the
## weather of a day that has been is not going to change, and 720 calls across
## the boundary every frame for a picture that moves once a day would be the one
## expensive thing on an otherwise cheap screen.
func refresh() -> void:
	if _republic == null:
		return
	var shape: PackedInt32Array = _republic.calendar()
	if shape.size() < 5:
		_notice.text = "no republic has been founded"
		return
	_notice.text = ""

	var days_per_year: int = shape[C_DAYS_PER_YEAR]
	var days_per_month: int = shape[C_DAYS_PER_MONTH]
	var today: int = shape[C_DAY_INDEX]
	var day_of_year: int = shape[C_DAY_OF_YEAR]
	var year_began: int = today - day_of_year

	# Days lived this year, plus the five the HUD already forecasts. `+ 1`
	# because today is a day that has been had.
	var lived := PackedFloat32Array()
	for i in day_of_year + 1:
		lived.append(_republic.temperature_on_day(year_began + i))
	var ahead := PackedFloat32Array()
	for i in range(1, FORECAST_DAYS + 1):
		if day_of_year + i >= days_per_year:
			break
		ahead.append(_republic.temperature_on_day(today + i))

	# Last year, whole, if the republic was here for it. A republic founded in
	# March has lived part of 1960 and none of 1959, and the days before the
	# founding are weather in an empty field -- so a previous year counts only
	# once the republic has been standing through the whole of one.
	var previous := PackedFloat32Array()
	if shape[C_DAYS_ELAPSED] >= day_of_year + days_per_year:
		for i in days_per_year:
			previous.append(_republic.temperature_on_day(year_began - days_per_year + i))

	_chart.set_year(lived, ahead, previous, days_per_year, days_per_month, MONTHS)
	_write(_this_year, lived, days_per_month)
	_write(_last_year, previous, days_per_month)


## The figures under the chart: the four things a fuel order is decided from.
##
## **A breakdown rather than a mean.** An average temperature is the one number
## that cannot be acted on -- a mild year with a hard fortnight in it and an
## evenly cool year average the same and cost completely different amounts of
## coal. What the player needs is the worst of it and how long it lasted.
func _write(into: VBoxContainer, temps: PackedFloat32Array, days_per_month: int) -> void:
	for child in into.get_children():
		if child is PanelContainer:
			child.queue_free()

	if temps.is_empty():
		var line := Parts.row(into)
		line.add_child(Parts.cell(Parts.say("nothing on record yet", "Faint"), 1.0))
		return

	var coldest: float = temps[0]
	var coldest_day := 0
	var warmest: float = temps[0]
	var warmest_day := 0
	var frost := 0
	# **The warm season, found as the longest run above freezing.** The obvious
	# figures -- first frost and last frost -- read "1 Jan" and "30 Dec" in every
	# climate cold enough to matter, because winter straddles the turn of the
	# year and the calendar cuts it in half. Measuring the summer instead puts
	# the two dates a fuel order is actually placed against on the screen: when
	# the thaw came and when the ground froze again.
	var run := 0
	var best := 0
	var best_end := -1
	for i in temps.size():
		var t: float = temps[i]
		if t < coldest:
			coldest = t
			coldest_day = i
		if t > warmest:
			warmest = t
			warmest_day = i
		if t < 0.0:
			frost += 1
			run = 0
			continue
		run += 1
		if run > best:
			best = run
			best_end = i

	var rows := [
		["coldest day", "%+.1f °C  ·  %s" % [coldest, _date_of(coldest_day, days_per_month)]],
		["warmest day", "%+.1f °C  ·  %s" % [warmest, _date_of(warmest_day, days_per_month)]],
		["days of frost", "%d of %d" % [frost, temps.size()]],
	]
	if best == 0:
		rows.append(["above freezing", "not one day"])
	elif best == temps.size():
		rows.append(["above freezing", "every day on record"])
	else:
		rows.append(["above freezing", "%s – %s" % [
			_date_of(best_end - best + 1, days_per_month),
			_date_of(best_end, days_per_month),
		]])

	var alt := false
	for row in rows:
		var line := Parts.row(into, alt)
		alt = not alt
		line.add_child(Parts.cell(Parts.say(String(row[0]), "Small"), 1.4))
		line.add_child(Parts.cell(
			Parts.figure(String(row[1])), 2.0, HORIZONTAL_ALIGNMENT_RIGHT
		))


## A day of the year as a date a player reads, rather than as "day 214".
func _date_of(day_of_year: int, days_per_month: int) -> String:
	if days_per_month <= 0:
		return str(day_of_year)
	@warning_ignore("integer_division")
	var month: int = day_of_year / days_per_month
	var day: int = day_of_year % days_per_month + 1
	return "%d %s" % [day, MONTHS[month % MONTHS.size()]]


## The curve itself.
##
## An inner class with a `_draw` rather than a texture or a pile of `ColorRect`s.
## A year is 360 points and the shape of it -- how deep the winter goes, how
## abruptly -- is the whole content; a table of 360 rows would carry the same
## figures and answer nothing. Nothing here chooses a colour that is not in the
## palette.
class Chart extends Control:
	const P := preload("res://ui/palette.gd")

	## Room on the left for the axis figures and under it for the months.
	const PAD_LEFT := 46
	const PAD_BOTTOM := 20
	const PAD_TOP := 10
	const PAD_RIGHT := 8

	var _lived := PackedFloat32Array()
	var _ahead := PackedFloat32Array()
	var _previous := PackedFloat32Array()
	var _days := 0
	var _month_days := 0
	var _months: Array = []

	func set_year(
		lived: PackedFloat32Array,
		ahead: PackedFloat32Array,
		previous: PackedFloat32Array,
		days: int,
		month_days: int,
		months: Array
	) -> void:
		_lived = lived
		_ahead = ahead
		_previous = previous
		_days = days
		_month_days = month_days
		_months = months
		queue_redraw()

	func _draw() -> void:
		if _days <= 0:
			return
		var plot := Rect2(
			PAD_LEFT, PAD_TOP,
			maxf(size.x - PAD_LEFT - PAD_RIGHT, 1.0),
			maxf(size.y - PAD_TOP - PAD_BOTTOM, 1.0)
		)
		var font := get_theme_font("font", "Label")
		var tiny := P.SIZE_TINY

		# **The scale is taken from the record, not fixed.** A tundra winter and
		# a temperate one differ by forty degrees, and a chart clamped to one
		# range would flatten the other into a straight line.
		var low := 0.0
		var high := 0.0
		for run in [_lived, _ahead, _previous]:
			for t in run:
				low = minf(low, t)
				high = maxf(high, t)
		low = floorf((low - 2.0) / 5.0) * 5.0
		high = ceilf((high + 2.0) / 5.0) * 5.0
		if high - low < 1.0:
			high = low + 1.0

		# Month gridlines and their names, so a dip in the line is a date.
		if _month_days > 0:
			@warning_ignore("integer_division")
			var months: int = _days / _month_days
			for m in months:
				var x: float = plot.position.x + plot.size.x * float(m * _month_days) / float(_days)
				draw_line(
					Vector2(x, plot.position.y),
					Vector2(x, plot.position.y + plot.size.y),
					P.RULE,
					1.0
				)
				if font != null and m < _months.size():
					draw_string(
						font,
						Vector2(x + 3.0, size.y - 6.0),
						String(_months[m]),
						HORIZONTAL_ALIGNMENT_LEFT,
						-1,
						tiny,
						P.PAPER_FAINT
					)

		# Every five degrees, labelled. Freezing is drawn strongest of all: it is
		# the line the whole screen is about, because heating and the ground both
		# change behaviour across it.
		var step := 5.0
		var mark := low
		while mark <= high + 0.01:
			var y: float = _y(mark, low, high, plot)
			var freezing: bool = absf(mark) < 0.01
			draw_line(
				Vector2(plot.position.x, y),
				Vector2(plot.position.x + plot.size.x, y),
				P.RULE_STRONG if freezing else P.RULE,
				2.0 if freezing else 1.0
			)
			if font != null:
				draw_string(
					font,
					Vector2(2.0, y + 4.0),
					"%+.0f" % mark,
					HORIZONTAL_ALIGNMENT_LEFT,
					PAD_LEFT - 8,
					tiny,
					P.PAPER_DIM if freezing else P.PAPER_FAINT
				)
			mark += step

		# Last year behind this one, faint, so the two are compared by looking
		# rather than by remembering.
		_curve(_previous, 0, low, high, plot, P.PAPER_FAINT, 1.0)
		_curve(_lived, 0, low, high, plot, P.OCHRE, 2.0)
		# The five days the HUD forecasts, dimmed and continuing from today, so
		# the eye can see where the record stops and the forecast starts.
		if _lived.size() > 0:
			var joined := PackedFloat32Array([_lived[_lived.size() - 1]])
			joined.append_array(_ahead)
			_curve(joined, _lived.size() - 1, low, high, plot, P.PAPER_DIM, 1.0)

	## One run of days, starting at `from`.
	func _curve(
		temps: PackedFloat32Array,
		from: int,
		low: float,
		high: float,
		plot: Rect2,
		tone: Color,
		width: float
	) -> void:
		if temps.size() < 2:
			return
		var points := PackedVector2Array()
		for i in temps.size():
			points.append(Vector2(
				plot.position.x + plot.size.x * float(from + i) / float(maxi(_days - 1, 1)),
				_y(temps[i], low, high, plot)
			))
		draw_polyline(points, tone, width, true)

	func _y(value: float, low: float, high: float, plot: Rect2) -> float:
		var t: float = clampf((value - low) / (high - low), 0.0, 1.0)
		return plot.position.y + plot.size.y * (1.0 - t)
