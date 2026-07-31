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
## now, while the surrounding screens are being written, than later. What it costs
## today is this file honestly saying there is nothing to play.
##
## It reads `user://radio` for `.ogg` and `.wav` files, so dropping a track in
## makes it appear with no code change. That is the whole content-drop claim, and
## the empty state below is what it looks like until somebody does.

const P := preload("res://ui/palette.gd")
const Parts := preload("res://ui/parts.gd")
const Sheet := preload("res://ui/sheet.gd")

## Where composed tracks go. Under `user://` rather than in the project, so a
## track can be added to an installed game.
const DIR := "user://radio"

const COLUMNS := [["programme", 5.0], ["", 1.2, HORIZONTAL_ALIGNMENT_RIGHT]]

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

	_player = AudioStreamPlayer.new()
	_player.bus = "Radio"
	_player.finished.connect(_on_finished)
	add_child(_player)

	var sheet: Dictionary = Sheet.build(
		self,
		"The State Radio",
		"State Committee for Broadcasting",
		"The programme, in name order."
	)
	var body: VBoxContainer = sheet["body"]

	_now_playing = Parts.say("silent", "Stamp")
	body.add_child(_now_playing)

	var table := VBoxContainer.new()
	table.size_flags_vertical = Control.SIZE_EXPAND_FILL
	table.add_theme_constant_override("separation", 0)
	body.add_child(table)
	Parts.head(table, COLUMNS)
	_list = Parts.scroller(table)

	Sheet.close_button(sheet["footer"], "BACK", func(): closed.emit())

	# Held rather than dropped, because `_rebuild` disables them when there is
	# nothing to play. A transport that responds to nothing is the same lie the
	# empty state exists to refuse, and the main menu answers it the same way by
	# disabling Continue with no republic in progress.
	_stop_button = Parts.button("STOP")
	_stop_button.pressed.connect(_stop)
	sheet["footer"].add_child(_stop_button)

	_next_button = Parts.button("NEXT")
	_next_button.pressed.connect(_on_finished)
	sheet["footer"].add_child(_next_button)


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
		# The honest empty state. Not a placeholder track and not a fake playlist:
		# this build genuinely has no music, and saying where music would go is
		# more use to a player than pretending.
		_list.add_child(Parts.gap(P.GAP_WIDE))
		_list.add_child(Parts.say("The programme is empty.", "Small"))
		_list.add_child(Parts.prose(
			"The State Radio Orchestra has not yet recorded anything. Composed tracks "
			+ "placed in the folder below appear here, in name order.",
			"Faint"
		))
		_list.add_child(Parts.gap(P.GAP))
		_list.add_child(Parts.say(ProjectSettings.globalize_path(DIR), "Figure"))
		_now_playing.text = "SILENT"
		return

	for i in _tracks.size():
		var line := Parts.row(_list, i % 2 == 1)
		line.add_child(Parts.cell(Parts.say(_title_of(_tracks[i]), "Small"), COLUMNS[0][1]))
		var actions := HBoxContainer.new()
		actions.alignment = BoxContainer.ALIGNMENT_END
		var play := Parts.button("PLAY", "Primary" if i == _current else "")
		play.pressed.connect(_play.bind(i))
		actions.add_child(play)
		line.add_child(Parts.cell(actions, COLUMNS[1][1]))


## A file name as a title. Underscores to spaces, extension dropped.
func _title_of(file_name: String) -> String:
	return file_name.get_basename().replace("_", " ").replace("-", " ")


func _play(index: int) -> void:
	if index < 0 or index >= _tracks.size():
		return
	var stream := load("%s/%s" % [DIR, _tracks[index]])
	if stream == null or not (stream is AudioStream):
		_now_playing.text = "%s COULD NOT BE PLAYED" % _tracks[index].to_upper()
		return
	_current = index
	_player.stream = stream
	_player.play()
	_now_playing.text = "NOW PLAYING: %s" % _title_of(_tracks[index]).to_upper()
	_rebuild()


func _stop() -> void:
	_player.stop()
	_current = -1
	_now_playing.text = "SILENT"
	_rebuild()


## Advance the programme. Wraps, so the radio does not stop at the end of the
## night.
func _on_finished() -> void:
	if _tracks.is_empty():
		return
	_play((_current + 1) % _tracks.size())
