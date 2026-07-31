using System;
using System.Globalization;
using Godot;

namespace RedRepublic.Ui;

/// <summary>One column of a table: what goes in it, and what share of the width.</summary>
public readonly record struct Column(string Name, float Weight, HorizontalAlignment Align);

/// <summary>
/// The pieces every screen is built out of.
/// </summary>
/// <remarks>
/// <para>
/// <b>Nothing here chooses a colour, a size or a border.</b> Those are in
/// <c>ui/theme.tres</c>, which the project applies to every Control before any of
/// this runs; a part is a <i>shape</i> — a row, a column head, a stepper — named
/// once so that nine screens cannot each invent their own.
/// </para>
/// <para>
/// The one thing a part may take is a theme <b>type variation</b> — "Field",
/// "Primary" — which is a name in the theme rather than a value. A caller that
/// needs a colour reads <see cref="Palette"/>, and the only callers that
/// legitimately do are the ones tinting a control by what the simulation says.
/// </para>
/// </remarks>
public static class Parts
{
    // ---- words ----

    /// <summary>
    /// A line of text in one of the theme's voices.
    /// </summary>
    /// <remarks>
    /// The variation names are the type scale: Title, Section, Field, Body,
    /// Small, Faint, Note, Figure, FigureBig, Stamp, Alarm, Good.
    /// </remarks>
    public static Label Say(string text, string voice = "Body") =>
        new() { Text = text, ThemeTypeVariation = voice };

    /// <summary>
    /// A run of prose, which wraps.
    /// </summary>
    /// <remarks>
    /// <b>A label does not wrap by default, so its minimum width is the whole
    /// sentence on one line.</b> Put one in a column and that column can never be
    /// narrower than the text, whatever stretch ratio it was given — which is how
    /// a column asking for twice the share gets half.
    /// </remarks>
    public static Label Prose(string text, string voice = "Body")
    {
        var label = Say(text, voice);
        label.AutowrapMode = TextServer.AutowrapMode.WordSmart;
        return label;
    }

    /// <summary>
    /// A column name or a label beside a figure.
    /// </summary>
    /// <remarks>
    /// The capitals are typed here rather than left to the caller, so a screen
    /// cannot half-apply the convention. The theme letterspaces them.
    /// </remarks>
    public static Label Field(string text) => Say(text.ToUpperInvariant(), "Field");

    /// <summary>
    /// A number. Monospaced and right-aligned, because a column of figures that
    /// is not both is a column that shuffles as it updates.
    /// </summary>
    public static Label Figure(string text, string voice = "Figure")
    {
        var label = Say(text, voice);
        label.HorizontalAlignment = HorizontalAlignment.Right;
        return label;
    }

    // ---- controls ----

    public static Button Press(string text, string kind = "")
    {
        var b = new Button { Text = text, CustomMinimumSize = new Vector2(0, Palette.ButtonHeight) };
        if (kind.Length > 0)
        {
            b.ThemeTypeVariation = kind;
        }

        return b;
    }

    /// <summary>
    /// A minus and a plus, handed the signed change.
    /// </summary>
    /// <remarks>
    /// Square and monospaced, so three of them in a row line up and none changes
    /// width when its glyph does.
    /// </remarks>
    public static HBoxContainer Stepper(Action<double> nudge, double step = 1.0)
    {
        ArgumentNullException.ThrowIfNull(nudge);
        var box = new HBoxContainer { Alignment = BoxContainer.AlignmentMode.End };
        box.AddThemeConstantOverride("separation", 2);

        foreach (var (glyph, by) in new[] { ("-", -step), ("+", step) })
        {
            var b = Press(glyph, "Step");
            b.CustomMinimumSize = new Vector2(Palette.StepButton, Palette.StepButton);
            b.Pressed += () => nudge(by);
            box.AddChild(b);
        }

        return box;
    }

    /// <summary>
    /// A box you tick. Two states, both spelled out — a form has boxes, it does
    /// not have switches.
    /// </summary>
    /// <remarks>
    /// <b>The caller writes the label, and that is not laziness.</b> Connecting a
    /// handler that captures the button is a reference cycle: the button holds
    /// the connection, the connection holds the closure, the closure holds the
    /// button, and it is reported as a leak at exit. A caller already handling
    /// the toggle to write the value can set the word in the same handler for
    /// nothing.
    /// </remarks>
    public static Button Toggle()
    {
        var b = Press(Word(false), "Toggle");
        b.ToggleMode = true;
        b.CustomMinimumSize = new Vector2(64, Palette.ButtonHeight);
        return b;
    }

    /// <summary>What a ticked box says. One place, so a screen cannot spell it two ways.</summary>
    public static string Word(bool on) => on ? "YES" : "NO";

    // ---- structure ----

    /// <summary>
    /// A spacer that eats whatever room is left, so what follows sits at the far
    /// end of its container.
    /// </summary>
    public static Control Fill() =>
        new() { SizeFlagsHorizontal = Control.SizeFlags.ExpandFill };

    /// <summary>
    /// A fixed gap. Prefer container separation; this is for the one place a
    /// section wants more air than the rhythm gives it.
    /// </summary>
    public static Control Gap(int pixels) =>
        new() { CustomMinimumSize = new Vector2(0, pixels) };

    public static HSeparator Rule() => new();

    /// <summary>
    /// A scrolling column. Returns the column; the scroller is its parent.
    /// </summary>
    /// <remarks>
    /// Horizontal scrolling is off on every list in this game: a table that can
    /// slide sideways is a table whose right-hand column can be invisible with
    /// nothing saying so.
    /// </remarks>
    public static VBoxContainer Scroller(Control into, int separation = 0)
    {
        ArgumentNullException.ThrowIfNull(into);
        var scroll = new ScrollContainer
        {
            SizeFlagsVertical = Control.SizeFlags.ExpandFill,
            SizeFlagsHorizontal = Control.SizeFlags.ExpandFill,
            HorizontalScrollMode = ScrollContainer.ScrollMode.Disabled,
        };

        into.AddChild(scroll);

        var column = new VBoxContainer { SizeFlagsHorizontal = Control.SizeFlags.ExpandFill };
        column.AddThemeConstantOverride("separation", separation);
        scroll.AddChild(column);
        return column;
    }

    /// <summary>
    /// One line of a table. Returns the box its cells go in.
    /// </summary>
    /// <remarks>
    /// <b>Ruled, not boxed.</b> A list of forty bordered rectangles reads as
    /// forty things; a ruled table reads as one thing with forty lines in it.
    /// The alternate tint is what lets an eye track across a wide row.
    /// </remarks>
    public static HBoxContainer Row(Control into, bool alt = false)
    {
        ArgumentNullException.ThrowIfNull(into);
        var panel = new PanelContainer
        {
            ThemeTypeVariation = alt ? "RowAlt" : "Row",
            SizeFlagsHorizontal = Control.SizeFlags.ExpandFill,
        };

        into.AddChild(panel);

        var line = new HBoxContainer
        {
            CustomMinimumSize = new Vector2(0, Palette.RowHeight),
            Alignment = BoxContainer.AlignmentMode.Center,
        };

        line.AddThemeConstantOverride("separation", Palette.GapWide);
        panel.AddChild(line);
        return line;
    }

    /// <summary>
    /// A boxed card: one thing a player is asked to read or to press. Returns the
    /// column its contents go in.
    /// </summary>
    /// <remarks>
    /// Use a row for anything that is one of many; a card is for something that
    /// stands alone.
    /// </remarks>
    public static VBoxContainer Card(Control into, bool chosen = false)
    {
        ArgumentNullException.ThrowIfNull(into);
        var panel = new PanelContainer
        {
            ThemeTypeVariation = chosen ? "CardChosen" : "Card",
            SizeFlagsHorizontal = Control.SizeFlags.ExpandFill,
        };

        into.AddChild(panel);

        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", Palette.GapTight);
        panel.AddChild(column);
        return column;
    }

    /// <summary>
    /// Put a control in a column of a table, at a share of the width.
    /// </summary>
    /// <remarks>
    /// Every row in a table must call this with the same weights as its head, or
    /// the columns do not line up — which is a thing no test can see and every
    /// rendered frame can.
    /// </remarks>
    public static Control Cell(
        Control control, float weight, HorizontalAlignment align = HorizontalAlignment.Left)
    {
        ArgumentNullException.ThrowIfNull(control);
        control.SizeFlagsHorizontal = Control.SizeFlags.ExpandFill;
        control.SizeFlagsStretchRatio = weight;
        if (control is Label label)
        {
            label.HorizontalAlignment = align;
        }

        return control;
    }

    /// <summary>
    /// The head of a table: the column names, at the weights the rows will use.
    /// </summary>
    /// <remarks>
    /// Taking the weights here and in <see cref="Cell"/> from the same array at
    /// the call site is what keeps a head and its rows in step.
    /// </remarks>
    public static void Head(Control into, params Column[] columns)
    {
        ArgumentNullException.ThrowIfNull(into);
        ArgumentNullException.ThrowIfNull(columns);

        var line = new HBoxContainer();
        line.AddThemeConstantOverride("separation", Palette.GapWide);

        // The head sits inside the same left and right padding a row has, or the
        // column names are offset from the figures under them by the row's
        // margin.
        var pad = new MarginContainer();
        pad.AddThemeConstantOverride("margin_left", 12);
        pad.AddThemeConstantOverride("margin_right", 12);
        pad.AddThemeConstantOverride("margin_bottom", 4);
        pad.AddChild(line);
        into.AddChild(pad);

        foreach (var column in columns)
        {
            line.AddChild(Cell(Field(column.Name), column.Weight, column.Align));
        }

        into.AddChild(Rule());
    }

    /// <summary>
    /// A titled column with a scrolling list in it. Returns the list.
    /// </summary>
    public static VBoxContainer Section(
        Control into, string title, string blurb, float weight = 1.0f)
    {
        ArgumentNullException.ThrowIfNull(into);
        var box = new VBoxContainer
        {
            SizeFlagsHorizontal = Control.SizeFlags.ExpandFill,
            SizeFlagsVertical = Control.SizeFlags.ExpandFill,
            SizeFlagsStretchRatio = weight,
        };

        box.AddThemeConstantOverride("separation", Palette.GapTight);
        into.AddChild(box);

        box.AddChild(Say(title.ToUpperInvariant(), "Section"));
        if (blurb.Length > 0)
        {
            box.AddChild(Prose(blurb, "Faint"));
        }

        box.AddChild(Gap(Palette.GapTight));
        return box;
    }

    /// <summary>
    /// Two columns side by side, each scrolling on its own.
    /// </summary>
    /// <remarks>
    /// Stacked, a list of sixteen fills the screen by itself and the half a
    /// player acts on sits below the fold with the right third of every row
    /// empty. That was found by looking at a rendered frame and at nothing else.
    /// </remarks>
    public static HBoxContainer Columns(Control into)
    {
        ArgumentNullException.ThrowIfNull(into);
        var box = new HBoxContainer
        {
            SizeFlagsVertical = Control.SizeFlags.ExpandFill,
            SizeFlagsHorizontal = Control.SizeFlags.ExpandFill,
            ClipContents = true,
        };

        box.AddThemeConstantOverride("separation", Palette.GapSection * 2);
        into.AddChild(box);
        return box;
    }

    // ---- numbers ----

    /// <summary>
    /// Group a whole number with spaces, so 2500000 reads as 2 500 000.
    /// </summary>
    /// <remarks>
    /// Spaces rather than commas: this is a republic counting roubles, and the
    /// space is the convention nearly everywhere that is not the one country this
    /// game is deliberately not set in.
    /// </remarks>
    public static string Thousands(double value)
    {
        var digits = ((long)Math.Abs(Math.Round(value)))
            .ToString(CultureInfo.InvariantCulture);

        var text = new System.Text.StringBuilder();
        for (var i = 0; i < digits.Length; i++)
        {
            if (i > 0 && (digits.Length - i) % 3 == 0)
            {
                text.Append(' ');
            }

            text.Append(digits[i]);
        }

        return value < 0.0 ? "-" + text : text.ToString();
    }

    /// <summary>A tonnage, at a sensible magnitude.</summary>
    public static string Bulk(double tonnes) => tonnes switch
    {
        >= 1_000_000.0 => $"{tonnes / 1_000_000.0:F1} Mt",
        >= 1_000.0 => $"{tonnes / 1_000.0:F0} kt",
        _ => $"{tonnes:F0} t",
    };

    /// <summary>A figure that is usually whole, printed without a pointless decimal.</summary>
    public static string Clean(double value) =>
        Math.Abs(value - Math.Round(value)) < 1e-9
            ? ((long)Math.Round(value)).ToString(CultureInfo.InvariantCulture)
            : value.ToString("F1", CultureInfo.InvariantCulture);
}
