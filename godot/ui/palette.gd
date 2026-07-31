extends RefCounted

## Every colour and every measurement the interface is made of, in one file.
##
## **This is the only place a colour is chosen.** `theme.tres` is generated from
## it by `tools/build_theme.gd`, and the handful of places that must tint a
## control from code -- a tonnage that has run out, a component that is failing --
## read the constants here rather than typing a triple. A second copy of a chosen
## colour is the copy that drifts, and this interface had five of them.
##
## Presentation only. Nothing in this file may change how a republic turns out.
##
## Deliberately no `class_name`, for the reason `looks.gd` gives: a global class
## is registered only by an editor import pass, so a freshly added one is
## invisible to `godot --path ...` until somebody re-imports. Callers `preload`.
##
## # The theme is "a form issued by a ministry"
##
## Everything below serves one idea: the player is an administrator reading
## returns and signing orders, and the screen is the paperwork. That is why there
## are no rounded corners, no icons, no gradients and no shadows -- a printed form
## has none of those. What it does have is a stamped title block, a rule under it,
## ruled tables with the figures in a monospace, and exactly one colour that means
## "this is the decision".
##
## The three inks it is printed in:
##
## - **red** for a decision or a refusal, and nothing else;
## - **ochre** for the headings inside a table, so a section reads as a section
##   without a second size of type;
## - **paper white** for everything a player reads.
##
## Alarm and good are deliberately close to red and to nothing: this game says
## which thing is worst rather than scoring you, so the palette has one shout in
## it and no traffic light.

# ---- ink: the grounds, darkest to lightest ----------------------------------

## The deepest ground. A full-screen modal, and the backdrop behind a menu.
const INK := Color(0.051, 0.059, 0.063)
## A panel: the sheet of paper a section is printed on.
const CARBON := Color(0.082, 0.094, 0.102)
## A row, a card, a control at rest -- one step up off the sheet.
const CARBON_RAISED := Color(0.110, 0.125, 0.137)
## A well: an input, a pressed button, a scroll track.
const CARBON_SUNK := Color(0.039, 0.047, 0.051)

## The hairline every box and every table row is ruled with.
const RULE := Color(0.180, 0.204, 0.220)
## A rule that has to be seen from across the screen: a focused control, the
## division between two halves of a screen.
const RULE_STRONG := Color(0.271, 0.302, 0.322)

# ---- the inks it is printed in ----------------------------------------------

## Anything a player reads. Warm rather than blue-white: this is newsprint under
## a desk lamp, not a terminal.
const PAPER := Color(0.894, 0.882, 0.855)
## A label beside a figure, a sentence of explanation.
const PAPER_DIM := Color(0.659, 0.651, 0.627)
## A unit, a caption, a thing that is present and is not being read.
const PAPER_FAINT := Color(0.435, 0.443, 0.451)

## Institutional red: the one action a screen exists for, and a refusal.
const RED := Color(0.702, 0.216, 0.169)
const RED_HOT := Color(0.824, 0.271, 0.212)

## A heading inside a table, and a stamp.
const OCHRE := Color(0.769, 0.635, 0.290)

## Something is wrong and the player can act on it.
const ALARM := Color(0.886, 0.463, 0.361)
## Something is being done well. Used sparingly -- see the note at the top.
const GOOD := Color(0.498, 0.663, 0.420)

# ---- measurement -------------------------------------------------------------
#
# Everything is a multiple of four, and most of it a multiple of eight. A form is
# ruled paper; nothing on it sits at an arbitrary offset.

const STEP := 4
const GAP_TIGHT := 4
const GAP := 8
const GAP_WIDE := 16
const GAP_SECTION := 24

## The margin inside a panel, and the margin round a full screen.
const PAD := 16
const MARGIN_SCREEN := 48
const MARGIN_SCREEN_TOP := 40

## A row in a table. Tall enough for a 15 px face with a rule under it and no more.
const ROW_HEIGHT := 30
const BUTTON_HEIGHT := 34
## A `-` or `+` beside a number.
const STEP_BUTTON := 28

# ---- type --------------------------------------------------------------------
#
# Three faces, each with one job. See `tools/fetch_fonts.py` for why they are PT.

const FACE_TEXT := "res://art/fonts/PTSans-Regular.ttf"
const FACE_TEXT_BOLD := "res://art/fonts/PTSans-Bold.ttf"
const FACE_TEXT_ITALIC := "res://art/fonts/PTSans-Italic.ttf"
const FACE_NARROW := "res://art/fonts/PTSansNarrow-Regular.ttf"
const FACE_NARROW_BOLD := "res://art/fonts/PTSansNarrow-Bold.ttf"
const FACE_FIGURE := "res://art/fonts/PTMono-Regular.ttf"

const SIZE_TITLE := 38
const SIZE_SECTION := 19
const SIZE_LABEL := 14
const SIZE_BODY := 15
const SIZE_SMALL := 13
const SIZE_FIGURE := 14
const SIZE_FIGURE_BIG := 21
const SIZE_TINY := 11
