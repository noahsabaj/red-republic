extends CanvasLayer

## The State Radio: plumbed, and deliberately empty.
##
## The soundtrack wants **real fixed composed songs** that replay identically, not
## generative improvisation -- which is a decision about content and taste, and
## nobody working on this repository can hear it to judge. So the panel, the
## playlist and the transport are built now and the tracks arrive later as a
## content drop rather than as a retrofit.
##
## # Why build an empty panel at all
##
## Because the retrofit is the expensive part. A radio bolted on after the fact
## needs a bus, a volume control, a place in the pause menu, a way to survive a
## save and somewhere to put the transport -- and every one of those is cheaper
## now, while the surrounding screens are being written, than later. What it
## costs today is this file honestly saying there is nothing to play.
##
## It reads `user://radio` for `.ogg` and `.wav` files, so dropping a track in
## makes it appear with no code change. That is the whole content-drop claim, and
## the empty state below is what it looks like until somebody does.

const Style := preload("res://ui/theme.gd")

## Where composed tracks go. Under `user://` rather than in the project, so a
## track can be added to an installed game.
const DIR := "user://radio"

signal closed

var _list: VBoxContainer = null
var _now_playing: Label = null
var _player: AudioStreamPlayer = null
var _tracks: Array[String] = []
var _stop_button: Button = null
var _next_button: Button = null
var _current := -1


func _ready() -> void:
	layer = 12
	add_child(Style.backdrop(0.95))

	_player = AudioStreamPlayer.new()
	_player.bus = "Radio"
	_player.finished.connect(_on_finished)
	add_child(_player)

	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 72)
	margin.add_theme_constant_override("margin_right", 72)
	margin.add_theme_constant_override("margin_top", 36)
	margin.add_theme_constant_override("margin_bottom", 28)
	add_child(margin)

	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 12)
	margin.add_child(rows)

	rows.add_child(Style.heading("THE STATE RADIO", Style.SIZE_TITLE))
	_now_playing = Style.body("", Style.ACCENT_HOT)
	rows.add_child(_now_playing)
	rows.add_child(Style.divider())

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	rows.add_child(scroll)
	_list = VBoxContainer.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.add_theme_constant_override("separation", 4)
	scroll.add_child(_list)

	rows.add_child(Style.divider())
	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", 8)

	# Held rather than dropped, because `_rebuild` disables them when there is
	# nothing to play. A transport that responds to nothing is the same lie the
	# empty state below exists to refuse, and the main menu already answers it
	# the same way by disabling Continue with no republic in progress.
	_stop_button = Style.button("Stop")
	_stop_button.pressed.connect(_stop)
	footer.add_child(_stop_button)

	_next_button = Style.button("Next")
	_next_button.pressed.connect(_on_finished)
	footer.add_child(_next_button)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer.add_child(spacer)

	var back := Style.button("Back")
	back.pressed.connect(func(): closed.emit())
	footer.add_child(back)
	rows.add_child(footer)


func open() -> void:
	_scan()
	_rebuild()


## Find what is installed. No manifest: a manifest is a second list to keep in
## step with the folder, and the folder is the truth.
func _scan() -> void:
	_tracks.clear()
	DirAccess.make_dir_recursive_absolute(DIR)
	var dir := DirAccess.open(DIR)
	if dir == null:
		return
	var found: Array[String] = []
	for file_name in dir.get_files():
		var lower := file_name.to_lower()
		if lower.ends_with(".ogg") or lower.ends_with(".wav"):
			found.append(file_name)
	# Sorted, so the programme is the same order every run. A radio whose running
	# order changed on each launch would make "play that one again" impossible.
	found.sort()
	_tracks = found


func _rebuild() -> void:
	for child in _list.get_children():
		child.queue_free()

	var silent := _tracks.is_empty()
	if _stop_button != null:
		_stop_button.disabled = silent
	if _next_button != null:
		_next_button.disabled = silent

	if silent:
		# The honest empty state. Not a placeholder track and not a fake
		# playlist: this build genuinely has no music, and saying where music
		# would go is more use to a player than pretending.
		_list.add_child(Style.body("The programme is empty.", Style.INK))
		_list.add_child(Style.small(
			"The State Radio Orchestra has not yet recorded anything. Composed "
			+ "tracks placed in the folder below appear here, in name order.",
			Style.INK_DIM
		))
		var gap := Control.new()
		gap.custom_minimum_size = Vector2(0, 8)
		_list.add_child(gap)
		_list.add_child(Style.small(
			ProjectSettings.globalize_path(DIR), Style.INK_FAINT
		))
		_now_playing.text = "silent"
		return

	for i in _tracks.size():
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 12)
		var title := Style.body(_title_of(_tracks[i]), Style.INK)
		title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(title)
		var play := Style.button("Play", i == _current)
		play.pressed.connect(_play.bind(i))
		row.add_child(play)
		_list.add_child(row)


## A file name as a title. Underscores to spaces, extension dropped.
func _title_of(file_name: String) -> String:
	return file_name.get_basename().replace("_", " ").replace("-", " ")


func _play(index: int) -> void:
	if index < 0 or index >= _tracks.size():
		return
	var stream := load("%s/%s" % [DIR, _tracks[index]])
	if stream == null or not (stream is AudioStream):
		_now_playing.text = "%s could not be played" % _tracks[index]
		return
	_current = index
	_player.stream = stream
	_player.play()
	_now_playing.text = "now playing: %s" % _title_of(_tracks[index])
	_rebuild()


func _stop() -> void:
	_player.stop()
	_current = -1
	_now_playing.text = "silent"
	_rebuild()


## Advance the programme. Wraps, so the radio does not stop at the end of the
## night.
func _on_finished() -> void:
	if _tracks.is_empty():
		return
	_play((_current + 1) % _tracks.size())
