extends RefCounted

## Where saves live, and what is in the folder.
##
## The bytes are the simulation crate's business and this file never looks
## inside one: `Republic.save_preview` reads what a save says about itself, which
## is why `SavePreview` sits ahead of the world in the format. This is purely the
## directory.
##
## # Saves are files, and the file name is not the truth
##
## A save's republic name comes out of the save, never out of its file name. The
## name in the file name is a convenience for anyone looking in the folder, and it
## is sanitised, deduplicated and therefore **lossy** -- a player who renames a
## file has renamed a file and not a republic. That distinction is why the name
## went into the format at all.

const DIR := "user://saves"

## Saves whose format this build cannot read are still listed, with the reason.
##
## Hiding them would be worse: a player whose republic has become unopenable
## needs to be told that rather than to watch it vanish from the list. M13 is
## where migrations get decided; until then the honest answer is a greyed row
## saying which build wrote it.
const UNREADABLE := "unreadable"


static func ensure_dir() -> void:
	DirAccess.make_dir_recursive_absolute(DIR)


## The real filesystem path of a save.
##
## **Globalized, and that is not cosmetic.** `user://` is Godot's own virtual
## filesystem and the Rust side reads and writes with `std::fs`, which has never
## heard of it — passing the virtual path through gave `os error 123`, the
## filename syntax is incorrect. Found by running the save check rather than by
## reading anything: every call typechecked and the string looked like a path.
##
## The translation belongs here because the shell is the only part of this project
## that knows Godot exists. A `std::fs` call that understood `user://` would be
## the simulation crate learning about an engine.
static func path_for(file_name: String) -> String:
	ensure_dir()
	return ProjectSettings.globalize_path("%s/%s" % [DIR, file_name])


## Every save in the folder, newest first.
##
## Each entry is `{file, path, name, date, day, population, climate, extent_km,
## modified, problem}`. `problem` is empty for a save that can be opened, and
## carries the refusal otherwise -- so a caller shows every row and greys the ones
## it cannot use, rather than deciding what to hide.
##
## Sorted by the file's modification time and not by in-game date, because "the
## one I was just playing" is what a player is looking for and two republics can
## sit on the same day.
static func listing(republic: Republic) -> Array:
	ensure_dir()
	var out: Array = []
	var dir := DirAccess.open(DIR)
	if dir == null:
		return out
	for file_name in dir.get_files():
		if not file_name.ends_with(".rrs"):
			continue
		var path := path_for(file_name)
		var preview: PackedStringArray = republic.save_preview(path)
		var row := {
			"file": file_name,
			"path": path,
			"modified": FileAccess.get_modified_time(path),
			"name": "",
			"date": "",
			"day": 0,
			"population": 0,
			"climate": 0,
			"extent_km": 0,
			"problem": "",
		}
		if preview.size() >= 6:
			row["name"] = preview[0]
			row["date"] = preview[1]
			row["day"] = int(preview[2])
			row["population"] = int(preview[3])
			row["climate"] = int(preview[4])
			row["extent_km"] = int(preview[5])
		elif preview.size() == 1:
			row["problem"] = preview[0]
		else:
			# The file was listed a moment ago and cannot be read now. Rare, and
			# not worth a special case beyond saying so.
			row["problem"] = "the file could not be read"
		out.append(row)

	out.sort_custom(func(a, b): return a["modified"] > b["modified"])
	return out


## A file name for a fresh save of this republic.
##
## `<name>-<date>.rrs`, sanitised, with a counter only if that collides. The date
## is the in-game one, because two saves of the same republic are told apart by
## where they are in its history and not by when the player was at their desk.
static func name_for(republic_name: String, date: String) -> String:
	var stem := sanitise(republic_name)
	if stem == "":
		stem = "republic"
	var candidate := "%s-%s.rrs" % [stem, date]
	if not FileAccess.file_exists(path_for(candidate)):
		return candidate
	var n := 2
	while FileAccess.file_exists(path_for("%s-%s-%d.rrs" % [stem, date, n])):
		n += 1
	return "%s-%s-%d.rrs" % [stem, date, n]


## The autosave file name. One per republic, overwritten.
##
## Deliberately a single rolling file rather than a numbered series. A series
## fills a folder with near-identical republics and makes the list useless for
## finding the save the player made on purpose; the manual saves are the history.
static func autosave_name(republic_name: String) -> String:
	var stem := sanitise(republic_name)
	return "%s-auto.rrs" % (stem if stem != "" else "republic")


## Reduce a republic's name to something every filesystem will take.
##
## Cyrillic is transliterated rather than stripped, because the naming register
## for this game is period Soviet and a republic called Железногорск would
## otherwise produce a file called `-.rrs`. Lossy on purpose -- see the module
## note on the file name not being the truth.
static func sanitise(text: String) -> String:
	var out := ""
	var lower := text.to_lower()
	for i in lower.length():
		var ch := lower[i]
		if TRANSLITERATE.has(ch):
			out += TRANSLITERATE[ch]
		elif (ch >= "a" and ch <= "z") or (ch >= "0" and ch <= "9"):
			out += ch
		elif ch == " " or ch == "-" or ch == "_":
			out += "-"
	# Collapse the runs a space-and-hyphen name produces, and trim the ends.
	while out.contains("--"):
		out = out.replace("--", "-")
	return out.trim_prefix("-").trim_suffix("-")


## Cyrillic to Latin, lower case only -- `sanitise` lowers first.
##
## The scheme is readability rather than any standard: this only has to make a
## folder browsable, and a player reading `zheleznogorsk-1961-04-11.rrs` has
## everything they need.
const TRANSLITERATE := {
	"а": "a", "б": "b", "в": "v", "г": "g", "д": "d", "е": "e", "ё": "e",
	"ж": "zh", "з": "z", "и": "i", "й": "y", "к": "k", "л": "l", "м": "m",
	"н": "n", "о": "o", "п": "p", "р": "r", "с": "s", "т": "t", "у": "u",
	"ф": "f", "х": "kh", "ц": "ts", "ч": "ch", "ш": "sh", "щ": "shch",
	"ъ": "", "ы": "y", "ь": "", "э": "e", "ю": "yu", "я": "ya",
}
