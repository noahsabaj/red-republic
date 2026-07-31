## Stockpiles, held as one flat array for the whole republic.
##
## Every building owns a run of [constant Tables.resources] slots inside a
## single `PackedFloat64Array`, so building `b`'s holding of resource `r` lives
## at `b * COUNT + r`. There is no `Stock` object and there is deliberately none:
## a stockpile per building is an allocation per building, and the production
## pass touches every one of them every tick.
##
## The operations are static and take the array by reference. `PackedFloat64Array`
## is a value type in GDScript, so they take it as an argument and mutate it in
## place rather than returning a copy — a `stock = Stock.add(stock, ...)` shape
## would copy the whole republic's stockpiles on every call.
##
## **A shortfall is a smaller delivery, never a debt.** Stock is clamped at zero
## rather than allowed to go negative, which is the archived build's rule and the
## reason a starved factory stalls instead of quietly poisoning every downstream
## rate with a negative tonnage.
class_name Stock
extends RefCounted

## Where building `b`'s run starts.
static func base(b: int) -> int:
	return b * Tables.resources.size()

## A fresh, empty set of stockpiles for `n` buildings.
static func make(n: int) -> PackedFloat64Array:
	var a := PackedFloat64Array()
	a.resize(n * Tables.resources.size())
	return a

static func get_at(stock: PackedFloat64Array, b: int, r: int) -> float:
	return stock[b * Tables.resources.size() + r]

static func set_at(stock: PackedFloat64Array, b: int, r: int, tonnes: float) -> void:
	stock[b * Tables.resources.size() + r] = maxf(0.0, tonnes)

static func add(stock: PackedFloat64Array, b: int, r: int, tonnes: float) -> void:
	var i := b * Tables.resources.size() + r
	stock[i] = maxf(0.0, stock[i] + tonnes)

## Take up to `tonnes`, returning what was actually there. Never goes negative.
static func take(stock: PackedFloat64Array, b: int, r: int, tonnes: float) -> float:
	var i := b * Tables.resources.size() + r
	var got := minf(stock[i], tonnes)
	stock[i] -= got
	return got

## Everything one building is holding, across all resources.
static func total(stock: PackedFloat64Array, b: int) -> float:
	var n := Tables.resources.size()
	var at := b * n
	var sum := 0.0
	for r in n:
		sum += stock[at + r]
	return sum

static func is_empty(stock: PackedFloat64Array, b: int) -> bool:
	return total(stock, b) <= 0.0

## Whether there is any of `r` at all. The check a production pass makes before
## deciding a building is starved.
static func has(stock: PackedFloat64Array, b: int, r: int) -> bool:
	return stock[b * Tables.resources.size() + r] > 0.0

## One building's whole run, copied out. For the interface, which wants a row to
## show; not for the systems, which index in place.
static func row(stock: PackedFloat64Array, b: int) -> PackedFloat64Array:
	var n := Tables.resources.size()
	return stock.slice(b * n, (b + 1) * n)
