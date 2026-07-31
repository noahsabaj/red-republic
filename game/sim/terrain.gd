## The ground of one republic, and the generator that shapes it.
##
## **Space is continuous; the grid describes terrain, not buildings.** There is
## no occupancy model here — a cell says what the ground is like at a place, and
## buildings sit at real positions with real footprints. The word "tile" does not
## belong in this vocabulary.
##
## # Heights are f32, deliberately, and that is load-bearing
##
## The Rust build stored heights and flow accumulation as `f32`, and the
## depression fill raises a hollow to **one f32 ulp** above its lip. Widening
## either to `f64` would generate a different landscape from the same seed,
## because an `f64` ulp is about 2²⁹ times smaller. So the arrays here are
## `PackedFloat32Array` and every `f32` operation is done as one `f64` operation
## stored straight back — see [Bits] for why that is exactly equivalent, and why
## it only holds for one operation per store.
##
## # Determinism
##
## Integer hashing plus polynomial interpolation: no transcendentals, no hash
## iteration order, nothing address-dependent. Map generation must reproduce
## across machines, because a shared seed is a promise between players.
class_name Terrain
extends RefCounted

enum Surface { GRASS, FOREST, ROCK, WATER }

## Cells along one side.
var cells: int = 0

## Metres across one sample cell. Carried on the map rather than read from a
## constant, so a save always knows the resolution it was written at instead of
## inheriting whatever the build currently defaults to.
var cell_size: float = 0.0

## Metres above the datum, one per cell, row-major. `f32` — see the class docs.
var height: PackedFloat32Array = PackedFloat32Array()

## One [enum Surface] per cell, row-major.
var surface: PackedByteArray = PackedByteArray()

static func is_buildable(s: int) -> bool:
	return s == Surface.GRASS or s == Surface.FOREST

static func is_walkable(s: int) -> bool:
	return s != Surface.WATER

## A flat grass square `extent` metres on a side — the test fixture, and the
## base a generator builds on.
static func flat(extent: float, size: float) -> Terrain:
	assert(size > 0.0, "a cell needs a positive size")
	assert(extent > 0.0, "a republic needs a positive extent")
	var t := Terrain.new()
	t.cell_size = size
	t.cells = int(ceil(extent / size))
	var count := t.cells * t.cells
	t.height = PackedFloat32Array()
	t.height.resize(count)
	t.surface = PackedByteArray()
	t.surface.resize(count)
	return t

## How far the map reaches, in metres.
func extent() -> float:
	return float(cells) * cell_size

func contains(x: float, y: float) -> bool:
	var e := extent()
	return x >= 0.0 and x < e and y >= 0.0 and y < e

## Index of the cell a point falls in, or -1 off the map.
func index_of(x: float, y: float) -> int:
	if not contains(x, y):
		return -1
	return int(y / cell_size) * cells + int(x / cell_size)

## The centre of a cell, in metres — the bridge from lattice back to the
## continuous world, and the only direction that conversion should ever go
## outside this file. Split into two calls because each axis depends on only one
## index, and returning a pair would allocate per cell in the generator's inner
## loop.
func cell_centre_x(cx: int) -> float:
	return (float(cx) + 0.5) * cell_size

func cell_centre_y(cy: int) -> float:
	return (float(cy) + 0.5) * cell_size

## The surface at a point, or -1 off the map.
func surface_at(x: float, y: float) -> int:
	var i := index_of(x, y)
	return -1 if i < 0 else surface[i]

## The height at a point in metres, or 0.0 off the map.
func height_at(x: float, y: float) -> float:
	var i := index_of(x, y)
	return 0.0 if i < 0 else height[i]

func set_surface_at(x: float, y: float, s: int) -> void:
	var i := index_of(x, y)
	if i >= 0:
		surface[i] = s

func set_height_at(x: float, y: float, h: float) -> void:
	var i := index_of(x, y)
	if i >= 0:
		height[i] = h

## Whether a straight run between two points crosses open water.
##
## Sampled at half-cell steps so a river cannot be stepped over. What a road
## order asks before it decides whether it is looking at a road or at a bridge —
## without it a gravel road could be laid straight across a river at the price
## of gravel.
func crosses_water(ax: float, ay: float, bx: float, by: float) -> bool:
	var d := Units.distance(ax, ay, bx, by)
	var steps := clampi(int(ceil(d / (cell_size * 0.5))), 1, 512)
	for step in range(steps + 1):
		var t := float(step) / float(steps)
		if surface_at(ax + (bx - ax) * t, ay + (by - ay) * t) == Surface.WATER:
			return true
	return false

## Whether every cell a rectangle touches is buildable — the check a placement
## makes. Samples on the cell lattice rather than testing corner points, so a
## footprint cannot straddle a lake by landing its corners on dry ground.
func area_is_buildable(cx: float, cy: float, width: float, depth: float) -> bool:
	var half_w := width / 2.0
	var half_d := depth / 2.0
	var min_x := cx - half_w
	var max_x := cx + half_w
	var min_y := cy - half_d
	var max_y := cy + half_d
	if min_x < 0.0 or min_y < 0.0:
		return false
	var e := extent()
	if max_x > e or max_y > e:
		return false
	var lo_x := int(min_x / cell_size)
	var hi_x := mini(int(ceil(max_x / cell_size)), cells)
	var lo_y := int(min_y / cell_size)
	var hi_y := mini(int(ceil(max_y / cell_size)), cells)
	for gy in range(lo_y, hi_y):
		var row := gy * cells
		for gx in range(lo_x, hi_x):
			if not is_buildable(surface[row + gx]):
				return false
	return true

## Fraction of the map covered by one surface — a candidate card wants to say
## how much of a posting is forest or water.
func fraction_of(s: int) -> float:
	if surface.is_empty():
		return 0.0
	var hits := 0
	for i in surface.size():
		if surface[i] == s:
			hits += 1
	return float(hits) / float(surface.size())

# ---- deterministic value noise ----

const HASH_A: int = -7046029254386353131  # 0x9E3779B97F4A7C15
const HASH_B: int = -4658895280553007687  # 0xBF58476D1CE4E5B9
const HASH_C: int = -7723592293110705685  # 0x94D049BB133111EB

static func hash_cell(seed: int, x: int, y: int) -> int:
	var h := seed
	h ^= x * HASH_A
	h = Bits.rotl(h, 29)
	h ^= y * HASH_B
	h = Bits.rotl(h, 31)
	h ^= Bits.ushr(h, 27)
	return h * HASH_C

## Lattice value in `[0, 1)`.
static func lattice(seed: int, x: int, y: int) -> float:
	const SCALE: float = 1.0 / float(1 << 53)
	return float(Bits.ushr(hash_cell(seed, x, y), 11)) * SCALE

## Smoothstep — a polynomial, so exact everywhere.
static func smooth(t: float) -> float:
	return t * t * (3.0 - 2.0 * t)

## One octave of value noise at a given feature size, in `[0, 1)`.
static func value_noise(seed: int, px: float, py: float, feature: float) -> float:
	var fx := px / feature
	var fy := py / feature
	var x0 := floorf(fx)
	var y0 := floorf(fy)
	var tx := smooth(fx - x0)
	var ty := smooth(fy - y0)
	var ix := int(x0)
	var iy := int(y0)

	var n00 := lattice(seed, ix, iy)
	var n10 := lattice(seed, ix + 1, iy)
	var n01 := lattice(seed, ix, iy + 1)
	var n11 := lattice(seed, ix + 1, iy + 1)

	var top := n00 + (n10 - n00) * tx
	var bottom := n01 + (n11 - n01) * tx
	return top + (bottom - top) * ty

## Several octaves summed — big shapes with small detail on top.
static func fractal_noise(seed: int, px: float, py: float, feature: float, octaves: int) -> float:
	var total := 0.0
	var amplitude := 1.0
	var sum := 0.0
	var size := feature
	for octave in octaves:
		total += value_noise(seed + octave, px, py, size) * amplitude
		sum += amplitude
		amplitude *= 0.5
		size *= 0.5
	return total / sum

## Generate the ground of a square map.
##
## The noise field is a continuous function of position, so changing the cell
## size re-samples the *same* landscape more or less finely rather than
## generating a different one.
static func generate(seed: int, extent_m: float) -> Terrain:
	var t := flat(extent_m, Tables.terrain_cell_size)
	var n := t.cells
	for cy in n:
		var py := t.cell_centre_y(cy)
		var row := cy * n
		for cx in n:
			var v := fractal_noise(
				seed, t.cell_centre_x(cx), py, Tables.terrain_feature_size, Tables.terrain_octaves
			)
			var i := row + cx
			if v < Tables.terrain_water_below:
				t.surface[i] = Surface.WATER
			elif v > Tables.terrain_rock_above:
				t.surface[i] = Surface.ROCK
			elif v > Tables.terrain_forest_above:
				t.surface[i] = Surface.FOREST
			else:
				t.surface[i] = Surface.GRASS
			t.height[i] = v * Tables.terrain_relief
	t.carve_rivers()
	return t

## Cut river channels by following the water downhill.
##
## Thresholding noise gives **lakes, not rivers**: isolated basins wherever the
## field dips below the water threshold, with nothing joining them. Measured
## across three seeds before this existed, a 6 km map had between 0% and 3.2%
## water in at most six disconnected bodies — a map on which a bridge almost
## never has a river to span and a barge has nowhere to go.
##
## A river is not a shape you draw, it is where the water ends up. So: every cell
## sheds its own rain, each shove goes to the lowest neighbour, and a cell
## carrying more than the catchment threshold is a channel. The result is
## dendritic and connected by construction.
func carve_rivers() -> void:
	var n := cells
	var count := n * n
	if count == 0:
		return

	# **Fill the hollows before following the water.** Measured on the first
	# version, which did not: 601 disconnected channels on a 6 km map, none
	# spanning more than 1.7 km. Multi-octave noise is full of local minima, so a
	# channel ran to the nearest pit a few hundred metres later and stopped.
	var filled := PackedFloat32Array()
	var uphill := PackedInt32Array()
	_fill_depressions(filled, uphill)

	# Every cell sheds its own rain, and passes on whatever reached it.
	var flow := PackedFloat32Array()
	flow.resize(count)
	flow.fill(1.0)
	# Which way each cell drains, kept so the channel can be carved along the
	# line the water actually takes rather than guessed at afterwards.
	var down := PackedInt32Array()
	down.resize(count)
	down.fill(-1)

	for k in range(uphill.size() - 1, -1, -1):
		var i := uphill[k]
		var x := i % n
		var y := i / n
		var here := filled[i]

		# The steepest way down. Ties keep the neighbour found first, and the
		# neighbours are walked in a fixed order — so two equally low ways down
		# always resolve the same way on any machine.
		var best := 0.0
		var best_at := -1
		for dy in range(-1, 2):
			var ny := y + dy
			if ny < 0 or ny >= n:
				continue
			for dx in range(-1, 2):
				if dx == 0 and dy == 0:
					continue
				var nx := x + dx
				if nx < 0 or nx >= n:
					continue
				var j := ny * n + nx
				var h := filled[j]
				if h >= here:
					continue
				if best_at < 0 or h < best:
					best = h
					best_at = j

		# Nowhere lower to go. After the fill this only happens at the map edge,
		# which is where the water leaves the republic.
		if best_at >= 0:
			# One f32 operation, stored straight back — see the class docs.
			flow[best_at] = flow[best_at] + flow[i]
			down[i] = best_at

	var river := Bits.to_f32(Tables.terrain_river_catchment * float(count))
	var broad := Bits.to_f32(Tables.terrain_broad_catchment * float(count))
	if river <= 0.0:
		return

	# Two passes, because widening reads the channel it is widening: marking as
	# we go would let a freshly widened cell seed further widening and the rivers
	# would creep outward across the map.
	var channels := PackedInt32Array()
	for i in count:
		if flow[i] >= river:
			channels.append(i)

	for i in channels:
		surface[i] = Surface.WATER

		# **A diagonal step is not a continuous river.** Water flows to any of
		# eight neighbours, so a channel running across the grain is a chain of
		# cells touching only at their corners — which reads as a river on a map
		# and is not one on the ground: a road crosses it between the corners
		# without ever being over water. Measured before this was here: 187
		# disconnected bodies on a 6 km map, which is one river reported as
		# thirty.
		var j := down[i]
		if j < 0 or j >= count:
			continue
		var x := i % n
		var y := i / n
		var jx := j % n
		var jy := j / n
		if jx != x and jy != y:
			# Of the two cells that square off the corner, take the lower — that
			# is the one the water would actually have cut through.
			var a := y * n + jx
			var b := jy * n + x
			surface[a if height[a] <= height[b] else b] = Surface.WATER

	for i in channels:
		if flow[i] < broad:
			continue
		var x := i % n
		var y := i / n
		for d: Array in [[-1, 0], [1, 0], [0, -1], [0, 1]]:
			var nx: int = x + d[0]
			var ny: int = y + d[1]
			if nx < 0 or ny < 0 or nx >= n or ny >= n:
				continue
			surface[ny * n + nx] = Surface.WATER

## Raise every hollow to the level of its lowest lip, so that from anywhere on
## the map the water can find its way to the edge.
##
## Priority-flood: start from the border, always take the lowest cell seen so
## far, and pull each neighbour up to at least that height. A cell can only be
## reached across the *lowest* rim between it and the outside, so the height it
## is pulled up to is exactly the level a real pond in that hollow would stand
## at.
##
## The terrain keeps its real heights — a filled basin is not a hill that got
## taller, it is a place where a lake would sit, and the map should still say how
## deep it is. `visited` comes back in ascending order of filled height, which is
## exactly the order flow accumulation needs, reversed; handing it back saves
## sorting a million cells for an answer this pass has already worked out.
##
## # Why a bucket queue and not a heap
##
## Priority-flood only ever pushes at a level at or above the one it is draining,
## so the frontier is **monotone** — the same property that lets Dijkstra with
## bounded integer weights use a bucket queue. Heights fall in a known range, so
## the levels are an array of queues walked once from the bottom and the pass is
## linear. The price is that two cells within a centimetre are treated as level,
## so a filled pond can stand up to a centimetre off where an exact fill would
## put it — against 120 m of relief, and only in deciding which way a river
## leaves a plateau.
func _fill_depressions(filled: PackedFloat32Array, visited: PackedInt32Array) -> void:
	## How finely the frontier is banded. A centimetre: fine enough that the fill
	## is exact for anything the simulation can see, coarse enough that the level
	## array stays small.
	const STEP: float = 0.01

	var n := cells
	var count := n * n
	filled.resize(count)
	for i in count:
		filled[i] = height[i]
	if count == 0:
		return

	var lowest := INF
	var highest := -INF
	for i in count:
		var h := height[i]
		if h < lowest:
			lowest = h
		if h > highest:
			highest = h

	# One past the top, so a cell raised to the highest lip still has a home.
	var levels := _level_of(highest, lowest, STEP) + 2

	var closed := PackedByteArray()
	closed.resize(count)
	var frontier: Array[PackedInt32Array] = []
	frontier.resize(levels)
	for l in levels:
		frontier[l] = PackedInt32Array()

	for k in n:
		for i: int in [k, (n - 1) * n + k, k * n, k * n + n - 1]:
			if closed[i] == 0:
				closed[i] = 1
				var l := mini(_level_of(filled[i], lowest, STEP), levels - 1)
				frontier[l].append(i)

	# Walk the levels from the bottom. A level can grow while it is being
	# drained — a neighbour pulled up to the current lip lands back in this same
	# band — so the inner loop indexes rather than iterates.
	for level in levels:
		var cursor := 0
		while cursor < frontier[level].size():
			var i := frontier[level][cursor]
			cursor += 1
			visited.append(i)
			var x := i % n
			var y := i / n
			var here := filled[i]
			# **A hair above, never level.** Raising a hollow to exactly its lip
			# makes the basin floor perfectly flat, and a flat has no lower
			# neighbour — so the water arrives in the middle of a filled lake and
			# stops, which is the failure this pass exists to fix moved one step
			# along. Because the fill spreads outward *from* the outlet, adding
			# one ulp per step leaves a gradient running back the way it came,
			# which is the way out.
			var lip := Bits.next_up_f32(here)
			for dy in range(-1, 2):
				var ny := y + dy
				if ny < 0 or ny >= n:
					continue
				for dx in range(-1, 2):
					if dx == 0 and dy == 0:
						continue
					var nx := x + dx
					if nx < 0 or nx >= n:
						continue
					var j := ny * n + nx
					if closed[j] != 0:
						continue
					closed[j] = 1
					if filled[j] < lip:
						filled[j] = lip
					var l := mini(_level_of(filled[j], lowest, STEP), levels - 1)
					frontier[l].append(j)
		# Done with this band; give the memory back rather than holding a million
		# cells' worth of queues to the end of the pass.
		frontier[level] = PackedInt32Array()

static func _level_of(h: float, lowest: float, step: float) -> int:
	var band := (h - lowest) / step
	return 0 if band <= 0.0 else int(band)
