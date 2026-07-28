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
