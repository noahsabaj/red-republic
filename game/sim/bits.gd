## The integer and float operations GDScript does not have.
##
## GDScript's `int` is a **signed** 64-bit integer with no unsigned right shift,
## and its `float` is always a double with no 32-bit scalar type. Both gaps sit
## directly under determinism rules, so both are filled here rather than worked
## around at each call site:
##
## - Worldgen hashes positions with unsigned 64-bit arithmetic. `>>` sign-extends,
##   so a hash with the top bit set comes out wrong in the high bits only —
##   which passes an eyeball and fails the vectors.
## - Terrain heights and flow accumulation are `f32`. Widening them to `f64`
##   would generate a *different landscape* from the same seed, because the
##   depression fill steps by one `f32` ulp and an `f64` ulp is about 2²⁹ times
##   smaller. The map would still be deterministic and it would not be the map
##   the seed promised.
##
## `f32` arithmetic is reproduced by keeping the values in a `PackedFloat32Array`
## and doing each single operation in `f64` before storing back. That is exactly
## equivalent: `f64` carries more than twice an `f32` mantissa, so the `f64`
## result of one operation on two `f32`s is exact, and rounding it once on store
## gives the same answer the hardware's `f32` op would. It only holds for **one**
## operation per store — chaining two before writing back rounds once where the
## original rounded twice.
class_name Bits
extends RefCounted

## The sign bit on its own. `-9223372036854775808` is a parse error as a
## literal: the positive half is out of range before the minus is applied.
const MIN: int = 1 << 63

## Logical right shift. `>>` on its own sign-extends.
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

## Sixteen hex digits of an `int`'s bit pattern, read as unsigned.
##
## `"%x" % v` will not do: it prints a negative number with a minus sign, and
## every value with the top bit set is negative here.
static func hex64(v: int) -> String:
	const DIGITS := "0123456789abcdef"
	var out := ""
	for i in 16:
		out += DIGITS[(v >> ((15 - i) * 4)) & 0xf]
	return out

## Sixteen hex digits back to an `int`, by nibble.
##
## Not `String.hex_to_int()`: a value with the top bit set is above `int64` max
## as an unsigned number, and a parser that clamps rather than wraps would
## quietly turn every such value into the same one.
static func from_hex64(s: String) -> int:
	var v := 0
	for i in s.length():
		v = (v << 4) | "0123456789abcdef".find(s[i].to_lower())
	return v

# ---- f32 ----

## The nearest `f32` to a double — what storing into a `PackedFloat32Array` does,
## available without needing an array to store it in.
static func to_f32(v: float) -> float:
	var b := PackedByteArray()
	b.resize(4)
	b.encode_float(0, v)
	return b.decode_float(0)

## The next `f32` above this one, in the direction of `+INF`.
##
## The depression fill raises a hollow to **one ulp above** its lip rather than
## level with it: raising it to exactly the lip makes the basin floor perfectly
## flat, a flat has no lower neighbour, and the water arrives in the middle of a
## filled lake and stops — which is the failure the fill exists to remove, moved
## one step along.
static func next_up_f32(v: float) -> float:
	var b := PackedByteArray()
	b.resize(4)
	b.encode_float(0, v)
	var bits := b.decode_u32(0)
	if v == 0.0:
		# Both zeros step to the smallest positive subnormal.
		bits = 1
	elif v > 0.0:
		bits += 1
	else:
		bits -= 1
	b.encode_u32(0, bits)
	return b.decode_float(0)
