extends Node3D

## The camera: free orbit with constrained tilt.
##
## Real-time is the thesis, so the view has to be worth watching a lorry cross a
## field at. That rules out a locked isometric angle and it rules out a top-down
## one, where a moving vehicle is a dot. What it does not rule out is stopping
## the player putting the camera underground or looking at the horizon from
## inside a hill, which is what the tilt clamp is for.
##
## The rig is the pivot; the camera hangs off it at `distance`. Orbiting rotates
## the rig, panning slides it across the ground plane, and zoom changes the
## boom. That keeps "where am I looking" and "how far away am I" separate, which
## is what makes the controls feel like they are attached to the ground.

## How far the camera can tilt from horizontal. Never flat, never straight down.
const MIN_PITCH := deg_to_rad(12.0)
const MAX_PITCH := deg_to_rad(82.0)

const MIN_DISTANCE := 40.0
const ZOOM_STEP := 1.15

## Metres a second at ground level, scaled by how high the camera is: panning at
## 20 m of altitude and at 4 km should both feel like moving the map, not like
## moving the camera.
const PAN_SPEED := 0.55
const ORBIT_SPEED := 0.006

@onready var camera: Camera3D = $Camera

## Emitted whenever the boom length changes.
##
## The sun listens, because Godot's `directional_shadow_max_distance` is measured
## from the camera and is not a property the light can work out for itself. It
## defaults to 100 m; this camera opens at about 1,080 m on a 6 km map, so for
## the whole of this project's life nothing in frame was ever inside the shadow
## range and not one shadow was ever drawn. Nothing errored, and every number was
## healthy -- see `main.gd::_fit_shadows`.
signal distance_changed(metres: float)

var _distance := 800.0
var _pitch := deg_to_rad(50.0)
var _yaw := 0.0
var _max_distance := 12000.0
var _extent := 6000.0
var _orbiting := false


## Point the camera at the republic, not at the middle of the map.
##
## Where a posting gets founded is decided by the geology -- the shallowest coal
## body -- so it is routinely nowhere near the centre. Framing the map means
## opening on empty ground with the town somewhere off the edge of interest.
func frame_map(extent_m: float, at_x: float, at_z: float) -> void:
	_extent = extent_m
	_max_distance = extent_m * 1.6
	position = Vector3(at_x, 0.0, at_z)
	# Close enough that the buildings read as buildings rather than as marks.
	_distance = maxf(300.0, extent_m * 0.18)
	_apply()


## How long the boom is. The sun needs it to size its shadow range.
func get_distance() -> float:
	return _distance


## Tilt the camera, in degrees above horizontal. Capture runs use this.
##
## The sky is only visible below about 25 degrees, and until this existed the
## pitch was fixed at 50 and could only be changed by dragging a mouse -- so
## `--shot` could not photograph the sky at all, and a whole sky shader was
## written, rendered and pronounced broken on the evidence of frames that
## contained none of it. Every pixel sampled was the below-horizon half of the
## dome, which is correctly a flat colour.
func set_pitch(degrees: float) -> void:
	_pitch = clampf(deg_to_rad(degrees), MIN_PITCH, MAX_PITCH)
	_apply()


## Put the camera at a chosen boom length. Used by capture runs so a look can be
## judged at the distance a player actually watches a lorry from, rather than
## from the altitude that frames a whole posting.
func set_distance(metres: float) -> void:
	_distance = clampf(metres, MIN_DISTANCE, _max_distance)
	_apply()


func _process(delta: float) -> void:
	var move := Vector3.ZERO
	if Input.is_key_pressed(KEY_W):
		move.z -= 1.0
	if Input.is_key_pressed(KEY_S):
		move.z += 1.0
	if Input.is_key_pressed(KEY_A):
		move.x -= 1.0
	if Input.is_key_pressed(KEY_D):
		move.x += 1.0
	if move != Vector3.ZERO:
		# Normalised so a diagonal is not faster than a straight line, and
		# rotated by the yaw so "forward" means forward on screen.
		move = move.normalized().rotated(Vector3.UP, _yaw)
		position += move * PAN_SPEED * _distance * delta
		_clamp_to_map()
		_apply()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_MIDDLE or event.button_index == MOUSE_BUTTON_RIGHT:
			_orbiting = event.pressed
		elif event.button_index == MOUSE_BUTTON_WHEEL_UP and event.pressed:
			_distance = max(MIN_DISTANCE, _distance / ZOOM_STEP)
			_apply()
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN and event.pressed:
			_distance = min(_max_distance, _distance * ZOOM_STEP)
			_apply()
	elif event is InputEventMouseMotion and _orbiting:
		_yaw -= event.relative.x * ORBIT_SPEED
		_pitch = clamp(_pitch + event.relative.y * ORBIT_SPEED, MIN_PITCH, MAX_PITCH)
		_apply()


func _clamp_to_map() -> void:
	# Half a map's worth of slack outside the border, so you can look in at the
	# frontier from beyond it without being able to lose the republic entirely.
	var slack := _extent * 0.5
	position.x = clamp(position.x, -slack, _extent + slack)
	position.z = clamp(position.z, -slack, _extent + slack)


func _apply() -> void:
	rotation = Vector3(0.0, _yaw, 0.0)
	# The camera hangs off the rig, so its position and orientation are both
	# LOCAL. look_at_from_position takes globals -- using it here aimed the
	# camera at world zero, which is the map's corner, and the republic sat off
	# in the bottom-right of the frame with half the screen empty. Pitching the
	# camera directly is both correct and simpler: a Godot camera looks down
	# its own -Z, so rotating -pitch about X points it at the pivot.
	camera.position = Vector3(0.0, sin(_pitch), cos(_pitch)) * _distance
	camera.rotation = Vector3(-_pitch, 0.0, 0.0)
	# Every path that moves the boom comes through here, which is why the signal
	# is emitted from `_apply` rather than from the four callers that change
	# `_distance`. One of them would eventually be added without it.
	distance_changed.emit(_distance)
