extends Node

## The republic's sound, driven from simulation state.
##
## Every sample is synthesised in Rust (`crates/red-republic-shell/src/audio.rs`),
## which is where the determinism and level checks live. This file is the mixer:
## it builds the buses, wraps the samples in streams, and decides how loud each
## bed should be given what the republic is doing.
##
## # The beds follow state, never the calendar
##
## Rain volume comes from today's precipitation and snow from what is lying, the
## same rule the heating demand answers to: weather is state and not a month, so a
## cold snap is an event you can be caught out by. Machinery follows how much
## industry is actually running, so a republic whose factories have stalled goes
## quiet -- which is information, and it arrives before the player opens a panel.
##
## # Levels move on the day boundary, not per frame
##
## The state these read from changes daily at most. A per-frame recalculation
## would be sixty times the work for an identical answer, which is the same
## event-driven discipline the building buffer and the overlays already follow.
## What *is* per-frame is the fade toward the target, because a level that
## snapped would click.

const Store := preload("res://settings_store.gd")

## The buses the settings screen controls. Created at runtime rather than shipped
## as a `default_bus_layout.tres`, because a hand-authored binary-ish resource is
## one more thing to get silently wrong -- and a missing bus fails as sound
## playing at full volume on Master, which is the worst way to find out.
const BUSES := ["Ambience", "Machinery", "Interface", "Radio"]

## How fast a bed reaches a new level, in volume-units per second.
##
## Slow enough that a passing shower does not sound like a switch, fast enough
## that stepping into a full-speed winter is not silent for ten seconds.
const FADE_PER_SECOND := 0.55

var _sounds: Node = null
var _store: RefCounted = null
var _streams := {}
var _beds := {}
var _one_shots: Array[AudioStreamPlayer] = []
var _next_one_shot := 0
var _last_day := ""

## Voices that run continuously, with the bus each sits on.
const BEDS := ["wind", "rain", "snow", "machinery", "engine"]

## How many one-shot players to keep. A pool rather than one, so a click during a
## confirm does not cut the confirm off; and a pool rather than one-per-sound, so
## rapid clicking does not spawn nodes.
const ONE_SHOT_VOICES := 6


func setup(sounds: Node, store: RefCounted) -> void:
	_sounds = sounds
	_store = store
	_make_buses()
	_build_streams()
	_build_players()
	_store.apply()


func _make_buses() -> void:
	for bus_name in BUSES:
		if AudioServer.get_bus_index(bus_name) >= 0:
			continue
		var index := AudioServer.bus_count
		AudioServer.add_bus(index)
		AudioServer.set_bus_name(index, bus_name)
		AudioServer.set_bus_send(index, "Master")


## Wrap each generated voice in a stream, once.
##
## Generation is not free and the samples never change, so this happens at startup
## and never again. `AudioStreamWAV` takes raw PCM directly, which is why the Rust
## side hands over bytes rather than anything cleverer.
func _build_streams() -> void:
	for i in _sounds.voice_count():
		var name: String = _sounds.voice_name(i)
		var stream := AudioStreamWAV.new()
		stream.format = AudioStreamWAV.FORMAT_16_BITS
		stream.mix_rate = _sounds.sample_rate()
		stream.stereo = false
		stream.data = _sounds.voice_samples(i)
		if _sounds.voice_loops(i):
			stream.loop_mode = AudioStreamWAV.LOOP_FORWARD
			stream.loop_begin = 0
			# The whole buffer. The Rust side already crossfaded the seam, so
			# looping the entire thing is correct -- a shorter loop point would
			# skip the part that was faded to join.
			@warning_ignore("integer_division")
			stream.loop_end = stream.data.size() / 2
		_streams[name] = {"stream": stream, "bus": String(_sounds.voice_bus(i))}


func _build_players() -> void:
	for bed in BEDS:
		if not _streams.has(bed):
			continue
		var player := AudioStreamPlayer.new()
		player.stream = _streams[bed]["stream"]
		player.bus = _streams[bed]["bus"]
		# Silent to begin with. A bed that started at full and faded down would
		# make every launch open with a blast of wind.
		player.volume_db = Store.decibels(0.0)
		add_child(player)
		player.play()
		_beds[bed] = {"player": player, "level": 0.0, "target": 0.0}

	for _i in ONE_SHOT_VOICES:
		var player := AudioStreamPlayer.new()
		player.bus = "Interface"
		add_child(player)
		_one_shots.append(player)


## Play an interface sound.
##
## Round-robin over the pool. A click that steals the player a confirm is using is
## a click that truncates the confirm, which sounds like a fault.
func play(voice: String) -> void:
	if not _streams.has(voice) or _one_shots.is_empty():
		return
	var player := _one_shots[_next_one_shot]
	_next_one_shot = (_next_one_shot + 1) % _one_shots.size()
	player.stream = _streams[voice]["stream"]
	player.bus = _streams[voice]["bus"]
	player.play()


## Silence every bed. For the menu, where there is no republic to listen to.
func quieten() -> void:
	for bed in _beds.keys():
		_beds[bed]["target"] = 0.0


## Re-read what the republic is doing, and aim the beds at it.
##
## Called on the day boundary. The caller decides when that is, because it already
## tracks the date for the overlays and two places watching the same clock is two
## places to get it wrong.
func follow(republic: Republic) -> void:
	if not republic.is_founded():
		quieten()
		return

	# Wind is always there, and it is stronger in the cold -- which is a weather
	# cue rather than a physical claim: what the player needs to feel is that a
	# January morning is a different place from a July one.
	var temperature: float = republic.temperature_c()
	_beds["wind"]["target"] = clampf(0.34 + (10.0 - temperature) / 60.0, 0.22, 0.72)

	# Rain and snow are exclusive in effect if not in state: falling water below
	# freezing is snow, and playing both would double the hiss.
	var rain: float = republic.precipitation_mm()
	var snow: PackedFloat32Array = republic.snow()
	var lying: float = snow[0] if snow.size() >= 1 else 0.0
	var freezing := temperature <= 0.0
	_beds["rain"]["target"] = 0.0 if freezing else clampf(rain / 14.0, 0.0, 1.0)
	# Lying snow damps everything rather than making a noise of its own, so the
	# snow bed is about *falling* snow -- which is why it reads precipitation and
	# not depth. Depth only decides whether there is snow to fall onto.
	_beds["snow"]["target"] = clampf(rain / 18.0, 0.0, 0.8) if freezing else 0.0
	if lying > 0.05 and not freezing:
		# A thaw: running water. Borrowed from the rain bed rather than given a
		# voice of its own, because it is a season and not a mechanic.
		_beds["rain"]["target"] = maxf(_beds["rain"]["target"], 0.12)

	# Industry, as a share of what is standing. A republic whose works have
	# stalled goes quiet, and that is information.
	var buildings: float = float(republic.building_count())
	var employed: float = float(republic.employed())
	var working := 0.0 if buildings <= 0.0 else clampf(employed / (buildings * 12.0), 0.0, 1.0)
	_beds["machinery"]["target"] = working * 0.85

	# Traffic, from how many vehicles are actually out rather than how many exist.
	# A depot full of parked lorries makes no noise.
	var fleet: PackedFloat32Array = republic.fleet_by_medium()
	var ways := fleet.size() / 2
	var out := 0.0
	for i in ways:
		out += fleet[ways + i]
	_beds["engine"]["target"] = clampf(out / 12.0, 0.0, 0.8)


## Move every bed toward its target. Per frame, because a level that jumps clicks.
func _process(delta: float) -> void:
	for bed in _beds.keys():
		var entry: Dictionary = _beds[bed]
		var level: float = entry["level"]
		var target: float = entry["target"]
		if is_equal_approx(level, target):
			continue
		var step := FADE_PER_SECOND * delta
		entry["level"] = move_toward(level, target, step)
		entry["player"].volume_db = Store.decibels(entry["level"])


## What the caller compares against to decide whether a day has turned.
func day_changed(date: String) -> bool:
	if date == _last_day:
		return false
	_last_day = date
	return true
