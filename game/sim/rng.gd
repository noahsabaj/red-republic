## The simulation's only source of randomness.
##
## **xoshiro256++, seeded by SplitMix64** — the same generator the Rust build
## used, drawing the same stream. Not a fresh choice: map generation must
## reproduce across machines because a shared seed is a promise between players,
## and `tests/test_rng.gd` checks this against vectors dumped from the Rust
## implementation rather than against its own word.
##
## # Signed arithmetic, unsigned algorithm
##
## GDScript's `int` is a **signed** 64-bit integer and it has no unsigned right
## shift, so half of xoshiro cannot be written directly:
##
## - `>>` sign-extends, so `x >> 11` on a value with the top bit set fills with
##   ones and every draw comes out wrong in the high bits only — which passes a
##   casual eyeball and fails the vectors.
## - `%` is a signed remainder, so `x % n` on a negative `x` returns a negative
##   number, and `next_bounded` would hand back indices below zero.
##
## Every such operation goes through a helper here. The bit patterns are
## identical to Rust's `u64`; only the sign GDScript *reads* them with differs,
## and nothing reads the sign.
class_name Rng
extends RefCounted

## The sign bit on its own. Writing `-9223372036854775808` as a literal is a
## parse error — the positive half is out of range before the minus is applied.
const MIN: int = 1 << 63

const SPLIT_GAMMA: int = -7046029254386353131  # 0x9E3779B97F4A7C15
const SPLIT_A: int = -4658895280553007687      # 0xBF58476D1CE4E5B9
const SPLIT_B: int = -7723592293110705685      # 0x94D049BB133111EB

var _s0: int
var _s1: int
var _s2: int
var _s3: int

## Expand a seed into a full state with SplitMix64, as the xoshiro authors
## specify. Seeding the state directly from a small integer leaves it mostly
## zero, and xoshiro needs several rounds to recover from that.
static func from_seed(seed: int) -> Rng:
	var r := Rng.new()
	var z := seed
	var out: Array[int] = []
	for _i in 4:
		z += SPLIT_GAMMA
		var x := z
		x = (x ^ ushr(x, 30)) * SPLIT_A
		x = (x ^ ushr(x, 27)) * SPLIT_B
		out.append(x ^ ushr(x, 31))
	r._s0 = out[0]
	r._s1 = out[1]
	r._s2 = out[2]
	r._s3 = out[3]
	return r

## The next 64 random bits — the primitive every other method is built on.
func next_u64() -> int:
	var result := rotl(_s0 + _s3, 23) + _s0
	var t := _s1 << 17
	_s2 ^= _s0
	_s3 ^= _s1
	_s1 ^= _s2
	_s0 ^= _s3
	_s2 ^= t
	_s3 = rotl(_s3, 45)
	return result

## A float in `[0, 1)`.
##
## Takes the top 53 bits — the exact width of an `f64` mantissa — and scales by
## 2⁻⁵³. Both steps are exact, so this introduces no rounding of its own. The
## shift must be logical: an arithmetic one leaves the value negative and the
## result outside the interval entirely.
func next_f64() -> float:
	const SCALE: float = 1.0 / float(1 << 53)
	return float(ushr(next_u64(), 11)) * SCALE

## A uniform integer in `[0, n)`, with the bias removed.
##
## Plain `next_u64() % n` is skewed toward small values whenever `n` does not
## divide 2⁶⁴. Rejecting the short leading block costs one extra draw
## vanishingly often and makes the distribution exact — worth it, because a bias
## here shows up as a map that subtly prefers one corner and takes a week to
## find.
##
## `n` must be positive and below 2³¹. The upper bound is not arbitrary: the
## unsigned remainder below splits the value into halves and multiplies them,
## and a larger `n` would overflow that product into the sign bit and return
## nonsense. Nothing in the simulation counts anything in billions, so the limit
## is stated and checked rather than worked around.
func next_bounded(n: int) -> int:
	assert(n > 0, "next_bounded requires a positive bound")
	assert(n < (1 << 31), "next_bounded is only exact below 2^31")
	# (2^64) mod n, which is what `(u64::MAX - n + 1) % n` computes.
	var m32 := 4294967296 % n
	var threshold := (m32 * m32) % n
	while true:
		var x := next_u64()
		if uge(x, threshold):
			return umod(x, n)
	return 0

## A float in `[lo, hi)`.
func next_range(lo: float, hi: float) -> float:
	return lo + next_f64() * (hi - lo)

## The current stream position. Persist this, not the seed: a save that restores
## the seed but not the position resumes a different future, and the failure is
## invisible until someone compares two runs.
func state() -> PackedInt64Array:
	return PackedInt64Array([_s0, _s1, _s2, _s3])

func set_state(s: PackedInt64Array) -> void:
	_s0 = s[0]
	_s1 = s[1]
	_s2 = s[2]
	_s3 = s[3]

# ---- unsigned primitives GDScript does not have ----

## Logical right shift: `>>` on its own sign-extends.
static func ushr(x: int, n: int) -> int:
	if n <= 0:
		return x
	if n >= 64:
		return 0
	return (x >> n) & ((1 << (64 - n)) - 1)

## Rotate left, treating the value as unsigned 64.
static func rotl(x: int, n: int) -> int:
	return (x << n) | ushr(x, 64 - n)

## `a >= b` with both read as unsigned. Flipping the sign bit on both maps the
## unsigned order onto the signed one.
static func uge(a: int, b: int) -> bool:
	return (a ^ MIN) >= (b ^ MIN)

## `x mod n` with `x` read as unsigned and `0 < n < 2^31`.
##
## Split into 32-bit halves so the arithmetic never leaves the positive range:
## `x = hi·2³² + lo`, so `x mod n = ((hi mod n)·(2³² mod n) + lo) mod n`. Both
## factors are below `n < 2³¹`, so the product stays under 2⁶² and cannot reach
## the sign bit.
static func umod(x: int, n: int) -> int:
	var hi := ushr(x, 32)
	var lo := x & 0xFFFFFFFF
	return ((hi % n) * (4294967296 % n) + lo) % n
