using Godot;

namespace RedRepublic.Ui;

/// <summary>
/// Builds <c>res://ui/theme.tres</c>, the one look every screen inherits.
/// </summary>
/// <remarks>
/// <para>
/// Run it with <c>--build-theme</c>. The generated resource is committed and the
/// project's default theme setting points at it, so the editor previews what the
/// game ships and a scene authored in the editor needs no styling of its own.
/// A checked-in generator, a committed artifact, and neither run as part of a
/// build.
/// </para>
/// <para>
/// <b>Why generated rather than hand-authored.</b> A theme is about thirty
/// style boxes, and in <c>.tres</c> each is nine lines of colour literals with a
/// generated id. Written by hand that is seven hundred lines nobody can read and
/// nobody can adjust: "the hairline is one shade lighter" means finding
/// twenty-six literals. Written here it is one constant in
/// <see cref="Palette"/>.
/// </para>
/// <para>
/// <b>What belongs here and what does not.</b> Here: anything true of every
/// button, every table row, every heading. In a screen: only what is true of
/// that screen. If a screen sets a colour, either this file is missing a
/// variation or the screen is wrong — with the one exception of tinting a
/// control by what the simulation says, which reads <see cref="Palette"/> and
/// never types a triple.
/// </para>
/// </remarks>
public static class ThemeBuilder
{
    public const string Path = "res://ui/theme.tres";

    public static Error Build()
    {
        var theme = new Theme();

        var text = Face(Palette.FaceText);
        var bold = Face(Palette.FaceTextBold);
        var italic = Face(Palette.FaceTextItalic);
        var figure = Face(Palette.FaceFigure);

        // Capitals want air between them or they set as a solid block. This is
        // the one typographic move the whole interface leans on, so it is a face
        // of its own rather than a per-label override.
        var narrow = Face(Palette.FaceNarrow, 1);
        var narrowBold = Face(Palette.FaceNarrowBold, 2);
        var narrowTitle = Face(Palette.FaceNarrowBold, 5);

        theme.DefaultFont = text;
        theme.DefaultFontSize = Palette.SizeBody;

        Labels(theme, text, italic, figure, narrow, narrowBold, narrowTitle);
        Buttons(theme, narrow, figure);
        Panels(theme);
        Inputs(theme, text, narrow);
        Bars(theme);
        Containers(theme);
        Rich(theme, text, bold, italic, figure);

        return ResourceSaver.Save(theme, Path);
    }

    /// <summary>
    /// A font, optionally letterspaced.
    /// </summary>
    /// <remarks>
    /// A font variation is how Godot spaces glyphs; the alternative is a
    /// per-label constant, which is the sort of thing that gets applied to eleven
    /// of twelve headings.
    /// </remarks>
    private static Font Face(string path, int spacing = 0)
    {
        var file = GD.Load<FontFile>(path);
        if (spacing == 0)
        {
            return file;
        }

        return new FontVariation { BaseFont = file, SpacingGlyph = spacing };
    }

    private static void Labels(
        Theme theme, Font text, Font italic, Font figure,
        Font narrow, Font narrowBold, Font narrowTitle)
    {
        // The plain label: what a label is if nobody said otherwise.
        theme.SetFont("font", "Label", text);
        theme.SetFontSize("font_size", "Label", Palette.SizeBody);
        theme.SetColor("font_color", "Label", Palette.Paper);
        theme.SetColor("font_outline_color", "Label", Palette.Ink);
        theme.SetConstant("outline_size", "Label", 0);
        theme.SetConstant("line_spacing", "Label", 4);

        // A table rather than eleven near-identical blocks, because the point of
        // a scale is that it is one decision.
        (string Name, Font Font, int Size, Color Colour)[] kinds =
        [
            ("Title", narrowTitle, Palette.SizeTitle, Palette.Paper),
            ("Section", narrowBold, Palette.SizeSection, Palette.Paper),
            ("Field", narrow, Palette.SizeLabel, Palette.PaperFaint),
            ("Body", text, Palette.SizeBody, Palette.PaperDim),
            ("Small", text, Palette.SizeSmall, Palette.PaperDim),
            ("Faint", text, Palette.SizeSmall, Palette.PaperFaint),
            ("Note", italic, Palette.SizeBody, Palette.PaperFaint),
            ("Figure", figure, Palette.SizeFigure, Palette.Paper),
            ("FigureBig", figure, Palette.SizeFigureBig, Palette.Paper),
            ("Stamp", narrowBold, Palette.SizeSmall, Palette.Ochre),
            ("Alarm", text, Palette.SizeSmall, Palette.Alarm),
            ("Good", text, Palette.SizeSmall, Palette.Good),
        ];

        foreach (var (name, font, size, colour) in kinds)
        {
            theme.SetTypeVariation(name, "Label");
            theme.SetFont("font", name, font);
            theme.SetFontSize("font_size", name, size);
            theme.SetColor("font_color", name, colour);
            theme.SetConstant("line_spacing", name, 4);
        }
    }

    private static void Buttons(Theme theme, Font narrow, Font figure)
    {
        theme.SetFont("font", "Button", narrow);
        theme.SetFontSize("font_size", "Button", Palette.SizeLabel);
        theme.SetColor("font_color", "Button", Palette.PaperDim);
        theme.SetColor("font_hover_color", "Button", Palette.Paper);
        theme.SetColor("font_pressed_color", "Button", Palette.Paper);
        theme.SetColor("font_focus_color", "Button", Palette.Paper);
        theme.SetColor("font_disabled_color", "Button", Palette.PaperFaint);
        theme.SetConstant("h_separation", "Button", Palette.Gap);
        theme.SetStylebox("normal", "Button", Box(Palette.CarbonRaised, Palette.Rule, 14, 8));
        theme.SetStylebox(
            "hover", "Button", Box(Palette.CarbonRaised.Lightened(0.06f), Palette.RuleStrong, 14, 8));
        theme.SetStylebox("pressed", "Button", Box(Palette.CarbonSunk, Palette.RuleStrong, 14, 8));
        theme.SetStylebox("disabled", "Button", Box(Palette.CarbonSunk, Palette.Rule, 14, 8));
        theme.SetStylebox("focus", "Button", Outline(Palette.RuleStrong, 14, 8));

        // <b>The one action a screen exists for.</b> At most one per screen: two
        // primary buttons is a screen that has not decided what it is for.
        theme.SetTypeVariation("Primary", "Button");
        theme.SetColor("font_color", "Primary", Palette.Paper);
        theme.SetColor("font_hover_color", "Primary", Colors.White);
        theme.SetColor("font_pressed_color", "Primary", Colors.White);
        theme.SetColor("font_disabled_color", "Primary", Palette.PaperFaint);
        theme.SetStylebox("normal", "Primary", Box(Palette.Red, Palette.Red, 18, 10));
        theme.SetStylebox("hover", "Primary", Box(Palette.RedHot, Palette.RedHot, 18, 10));
        theme.SetStylebox(
            "pressed", "Primary", Box(Palette.Red.Darkened(0.2f), Palette.RedHot, 18, 10));
        theme.SetStylebox("disabled", "Primary", Box(Palette.CarbonSunk, Palette.Rule, 18, 10));
        theme.SetStylebox("focus", "Primary", Outline(Palette.Paper, 18, 10));

        // A control that is present and is not the point: Back, Cancel, a tab
        // that is not the open one. No box at all until the cursor is on it.
        theme.SetTypeVariation("Quiet", "Button");
        theme.SetColor("font_color", "Quiet", Palette.PaperFaint);
        theme.SetStylebox("normal", "Quiet", Box(Colors.Transparent, Colors.Transparent, 10, 6));
        theme.SetStylebox("hover", "Quiet", Box(Palette.CarbonRaised, Palette.Rule, 10, 6));
        theme.SetStylebox("pressed", "Quiet", Box(Palette.CarbonSunk, Palette.Rule, 10, 6));
        theme.SetStylebox("disabled", "Quiet", Box(Colors.Transparent, Colors.Transparent, 10, 6));
        theme.SetStylebox("focus", "Quiet", Outline(Palette.RuleStrong, 10, 6));

        // A tab: an index card in a file. Nothing but a red rule under the open
        // one, which is the only piece of chrome that says where you are.
        theme.SetTypeVariation("Tab", "Button");
        theme.SetColor("font_color", "Tab", Palette.PaperFaint);
        theme.SetColor("font_hover_color", "Tab", Palette.Paper);
        theme.SetColor("font_pressed_color", "Tab", Palette.Paper);
        theme.SetStylebox("normal", "Tab", Underline(Colors.Transparent, Palette.Rule, 14, 9));
        theme.SetStylebox("hover", "Tab", Underline(Colors.Transparent, Palette.RuleStrong, 14, 9));
        theme.SetStylebox("pressed", "Tab", Underline(Palette.CarbonRaised, Palette.Red, 14, 9));
        theme.SetStylebox("focus", "Tab", Outline(Palette.RuleStrong, 14, 9));

        // A minus, a plus, a reset. Square, monospaced, and the same width
        // whatever is in it — three steppers in a row must not shuffle as their
        // glyphs change.
        theme.SetTypeVariation("Step", "Button");
        theme.SetFont("font", "Step", figure);
        theme.SetFontSize("font_size", "Step", Palette.SizeFigure);
        theme.SetColor("font_color", "Step", Palette.PaperDim);
        theme.SetColor("font_hover_color", "Step", Palette.Paper);
        theme.SetStylebox("normal", "Step", Box(Palette.CarbonRaised, Palette.Rule, 8, 5));
        theme.SetStylebox(
            "hover", "Step", Box(Palette.CarbonRaised.Lightened(0.08f), Palette.RuleStrong, 8, 5));
        theme.SetStylebox("pressed", "Step", Box(Palette.CarbonSunk, Palette.RuleStrong, 8, 5));
        theme.SetStylebox("disabled", "Step", Box(Palette.CarbonSunk, Palette.Rule, 8, 5));
        theme.SetStylebox("focus", "Step", Outline(Palette.RuleStrong, 8, 5));

        // A box you tick. <b>A form has boxes; it does not have switches</b> —
        // an iOS-shaped toggle in a ministry is the one thing on the screen that
        // would look imported.
        theme.SetTypeVariation("Toggle", "Button");
        theme.SetColor("font_color", "Toggle", Palette.PaperFaint);
        theme.SetColor("font_pressed_color", "Toggle", Palette.Paper);
        theme.SetColor("font_hover_color", "Toggle", Palette.Paper);
        theme.SetStylebox("normal", "Toggle", Box(Palette.CarbonSunk, Palette.Rule, 12, 7));
        theme.SetStylebox("hover", "Toggle", Box(Palette.CarbonRaised, Palette.RuleStrong, 12, 7));
        theme.SetStylebox(
            "pressed", "Toggle", Box(Palette.Red.Darkened(0.45f), Palette.Red, 12, 7));
        theme.SetStylebox("disabled", "Toggle", Box(Palette.CarbonSunk, Palette.Rule, 12, 7));
        theme.SetStylebox("focus", "Toggle", Outline(Palette.RuleStrong, 12, 7));
    }

    private static void Panels(Theme theme)
    {
        // The sheet a section is printed on.
        theme.SetStylebox(
            "panel", "PanelContainer", Box(Palette.Carbon, Palette.Rule, Palette.Pad, Palette.Pad));
        theme.SetStylebox(
            "panel", "Panel", Box(Palette.Carbon, Palette.Rule, Palette.Pad, Palette.Pad));

        // A card: one thing, in a box, a player is asked to read or to press.
        theme.SetTypeVariation("Card", "PanelContainer");
        theme.SetStylebox("panel", "Card", Box(Palette.CarbonRaised, Palette.Rule, 12, 10));

        // The one card chosen out of a row of them. <b>Exactly the padding Card
        // has</b>, because a chosen card with a different border width is one
        // whose contents sit a pixel higher than its neighbours'.
        theme.SetTypeVariation("CardChosen", "PanelContainer");
        theme.SetStylebox("panel", "CardChosen", Box(Palette.CarbonRaised, Palette.Red, 12, 10));

        // <b>A table row, not a card.</b> Rows are ruled, not boxed: a list of
        // forty bordered rectangles is forty things, and a ruled table is one
        // thing with forty lines in it.
        theme.SetTypeVariation("Row", "PanelContainer");
        theme.SetStylebox("panel", "Row", Underline(Palette.Carbon, Palette.Rule, 12, 7));
        theme.SetTypeVariation("RowAlt", "PanelContainer");
        theme.SetStylebox("panel", "RowAlt", Underline(Palette.CarbonRaised, Palette.Rule, 12, 7));

        theme.SetTypeVariation("RowHot", "PanelContainer");
        theme.SetStylebox(
            "panel", "RowHot",
            Underline(Palette.CarbonRaised.Lightened(0.05f), Palette.RuleStrong, 12, 7));
        theme.SetTypeVariation("RowChosen", "PanelContainer");
        theme.SetStylebox("panel", "RowChosen", Box(Palette.CarbonRaised, Palette.Red, 12, 7));

        // A well: something read out of the republic rather than typed into it.
        theme.SetTypeVariation("Well", "PanelContainer");
        theme.SetStylebox("panel", "Well", Box(Palette.CarbonSunk, Palette.Rule, 12, 10));

        // The title block at the top of every screen. No fill and a red rule
        // under it — the one piece of chrome that is the same on every screen.
        theme.SetTypeVariation("Header", "PanelContainer");
        theme.SetStylebox("panel", "Header", RuleUnder(Palette.Red, 2, 0, 10));

        // The HUD's panels, which sit over the world rather than a backdrop.
        // Nearly opaque: a number you cannot read because a lorry drove behind
        // it is not a number.
        theme.SetTypeVariation("Instrument", "PanelContainer");
        theme.SetStylebox(
            "panel", "Instrument",
            Box(new Color(Palette.Carbon, 0.93f), Palette.Rule, 12, 9));
    }

    private static void Inputs(Theme theme, Font text, Font narrow)
    {
        theme.SetFont("font", "LineEdit", text);
        theme.SetFontSize("font_size", "LineEdit", Palette.SizeBody);
        theme.SetColor("font_color", "LineEdit", Palette.Paper);
        theme.SetColor("font_placeholder_color", "LineEdit", Palette.PaperFaint);
        theme.SetColor("font_selected_color", "LineEdit", Palette.Ink);
        theme.SetColor("selection_color", "LineEdit", Palette.Ochre);
        theme.SetColor("caret_color", "LineEdit", Palette.RedHot);

        // A field on a form is a ruled line you write on, so the rule under it is
        // the whole control and the box round it is nearly invisible.
        theme.SetStylebox(
            "normal", "LineEdit", Underline(Palette.CarbonSunk, Palette.RuleStrong, 12, 8));
        theme.SetStylebox("focus", "LineEdit", Underline(Palette.CarbonSunk, Palette.Red, 12, 8));
        theme.SetStylebox(
            "read_only", "LineEdit", Underline(Palette.CarbonSunk, Palette.Rule, 12, 8));

        theme.SetFont("font", "OptionButton", narrow);
        theme.SetFontSize("font_size", "OptionButton", Palette.SizeLabel);
        theme.SetColor("font_color", "OptionButton", Palette.Paper);
        theme.SetColor("font_hover_color", "OptionButton", Colors.White);
        theme.SetColor("font_pressed_color", "OptionButton", Palette.Paper);
        theme.SetColor("font_focus_color", "OptionButton", Palette.Paper);
        theme.SetColor("font_disabled_color", "OptionButton", Palette.PaperFaint);

        // So the little triangle is the same ink as the words beside it rather
        // than the engine's default grey.
        theme.SetConstant("modulate_arrow", "OptionButton", 1);
        theme.SetConstant("arrow_margin", "OptionButton", Palette.Gap);
        theme.SetStylebox(
            "normal", "OptionButton", Underline(Palette.CarbonSunk, Palette.RuleStrong, 12, 8));
        theme.SetStylebox(
            "hover", "OptionButton", Underline(Palette.CarbonRaised, Palette.PaperFaint, 12, 8));
        theme.SetStylebox(
            "pressed", "OptionButton", Underline(Palette.CarbonSunk, Palette.Red, 12, 8));
        theme.SetStylebox(
            "disabled", "OptionButton", Underline(Palette.CarbonSunk, Palette.Rule, 12, 8));
        theme.SetStylebox("focus", "OptionButton", Outline(Palette.RuleStrong, 12, 8));

        theme.SetFont("font", "PopupMenu", text);
        theme.SetFontSize("font_size", "PopupMenu", Palette.SizeBody);
        theme.SetColor("font_color", "PopupMenu", Palette.PaperDim);
        theme.SetColor("font_hover_color", "PopupMenu", Palette.Paper);
        theme.SetColor("font_disabled_color", "PopupMenu", Palette.PaperFaint);
        theme.SetColor("font_separator_color", "PopupMenu", Palette.PaperFaint);
        theme.SetConstant("v_separation", "PopupMenu", 2);
        theme.SetConstant("item_start_padding", "PopupMenu", Palette.Gap);
        theme.SetConstant("item_end_padding", "PopupMenu", Palette.Gap);
        theme.SetStylebox("panel", "PopupMenu", Box(Palette.Carbon, Palette.RuleStrong, 4, 4));
        theme.SetStylebox(
            "hover", "PopupMenu", Box(Palette.CarbonRaised, Palette.CarbonRaised, 4, 4));
        theme.SetStylebox(
            "separator", "PopupMenu", new StyleBoxLine { Color = Palette.Rule, Thickness = 1 });

        // A slider is a gauge on an instrument: a sunk track with a red bar in it.
        theme.SetStylebox("slider", "HSlider", Bar(Palette.CarbonSunk, 6));
        theme.SetStylebox("grabber_area", "HSlider", Bar(Palette.Red, 6));
        theme.SetStylebox("grabber_area_highlight", "HSlider", Bar(Palette.RedHot, 6));
        theme.SetIcon("grabber", "HSlider", Mark(Palette.Paper, 8, 20));
        theme.SetIcon("grabber_highlight", "HSlider", Mark(Colors.White, 8, 20));
        theme.SetIcon("grabber_disabled", "HSlider", Mark(Palette.PaperFaint, 8, 20));
        theme.SetConstant("center_grabber", "HSlider", 1);
    }

    private static void Bars(Theme theme)
    {
        // Slim, and part of the ruling rather than a control in its own right.
        foreach (var bar in new[] { "VScrollBar", "HScrollBar" })
        {
            theme.SetStylebox("scroll", bar, Bar(Palette.CarbonSunk, 6));
            theme.SetStylebox("grabber", bar, Bar(Palette.RuleStrong, 6));
            theme.SetStylebox("grabber_highlight", bar, Bar(Palette.PaperFaint, 6));
            theme.SetStylebox("grabber_pressed", bar, Bar(Palette.PaperDim, 6));
        }

        theme.SetStylebox(
            "separator", "HSeparator", new StyleBoxLine { Color = Palette.Rule, Thickness = 1 });
        theme.SetStylebox(
            "separator", "VSeparator",
            new StyleBoxLine { Color = Palette.Rule, Thickness = 1, Vertical = true });
        theme.SetConstant("separation", "HSeparator", Palette.Gap);
        theme.SetConstant("separation", "VSeparator", Palette.Gap);

        theme.SetStylebox("background", "ProgressBar", Bar(Palette.CarbonSunk, 8));
        theme.SetStylebox("fill", "ProgressBar", Bar(Palette.Red, 8));
        theme.SetColor("font_color", "ProgressBar", Palette.Paper);

        // The scroll viewport itself has no box: the panel it sits in already has
        // one, and two nested borders is a dialog inside a dialog.
        theme.SetStylebox("panel", "ScrollContainer", new StyleBoxEmpty());
    }

    private static void Containers(Theme theme)
    {
        theme.SetConstant("separation", "BoxContainer", Palette.Gap);
        theme.SetConstant("separation", "HBoxContainer", Palette.Gap);
        theme.SetConstant("separation", "VBoxContainer", Palette.Gap);
        theme.SetConstant("h_separation", "GridContainer", Palette.GapWide);
        theme.SetConstant("v_separation", "GridContainer", 3);
        theme.SetConstant("h_separation", "FlowContainer", Palette.Gap);
        theme.SetConstant("v_separation", "FlowContainer", Palette.Gap);
    }

    private static void Rich(Theme theme, Font text, Font bold, Font italic, Font figure)
    {
        theme.SetFont("normal_font", "RichTextLabel", text);
        theme.SetFont("bold_font", "RichTextLabel", bold);
        theme.SetFont("italics_font", "RichTextLabel", italic);
        theme.SetFont("mono_font", "RichTextLabel", figure);
        theme.SetFontSize("normal_font_size", "RichTextLabel", Palette.SizeBody);
        theme.SetFontSize("bold_font_size", "RichTextLabel", Palette.SizeBody);
        theme.SetFontSize("italics_font_size", "RichTextLabel", Palette.SizeBody);
        theme.SetFontSize("mono_font_size", "RichTextLabel", Palette.SizeFigure);
        theme.SetColor("default_color", "RichTextLabel", Palette.PaperDim);
        theme.SetConstant("line_separation", "RichTextLabel", 4);
        theme.SetStylebox("normal", "RichTextLabel", new StyleBoxEmpty());
        theme.SetStylebox("focus", "RichTextLabel", new StyleBoxEmpty());
    }

    // ---- the shapes everything above is made of ----

    /// <summary>
    /// A filled box with a hairline round it. Square corners, always: a rounded
    /// corner is a phone application and this is a state instrument.
    /// </summary>
    private static StyleBoxFlat Box(Color fill, Color border, int padX, int padY)
    {
        var box = new StyleBoxFlat
        {
            BgColor = fill,
            BorderColor = border,
            ContentMarginLeft = padX,
            ContentMarginRight = padX,
            ContentMarginTop = padY,
            ContentMarginBottom = padY,
        };

        box.SetBorderWidthAll(1);
        return box;
    }

    /// <summary>A box ruled along the bottom only — a row in a table, or a field on a form.</summary>
    private static StyleBoxFlat Underline(Color fill, Color border, int padX, int padY)
    {
        var box = Box(fill, border, padX, padY);
        box.SetBorderWidthAll(0);
        box.BorderWidthBottom = 1;
        return box;
    }

    /// <summary>Nothing but a rule, of a given weight, with air under it.</summary>
    private static StyleBoxFlat RuleUnder(Color colour, int weight, int padX, int padY)
    {
        var box = Box(Colors.Transparent, colour, padX, padY);
        box.SetBorderWidthAll(0);
        box.BorderWidthBottom = weight;
        return box;
    }

    /// <summary>An outline and no fill: what focus looks like.</summary>
    private static StyleBoxFlat Outline(Color colour, int padX, int padY) =>
        Box(Colors.Transparent, colour, padX, padY);

    /// <summary>A track or a fill for a slider or a bar.</summary>
    private static StyleBoxFlat Bar(Color colour, int across) =>
        new()
        {
            BgColor = colour,
            ContentMarginTop = across / 2.0f,
            ContentMarginBottom = across / 2.0f,
            ContentMarginLeft = across / 2.0f,
            ContentMarginRight = across / 2.0f,
        };

    /// <summary>
    /// A plain rectangle of one colour, for the few theme items Godot wants a
    /// texture for rather than a box.
    /// </summary>
    /// <remarks>
    /// A gradient texture, because it serialises to text: a <c>.tres</c> that
    /// referenced a generated <c>.png</c> would be a theme with a loose file
    /// beside it that nothing regenerates.
    /// </remarks>
    private static GradientTexture2D Mark(Color colour, int wide, int tall)
    {
        var ramp = new Gradient();
        ramp.SetColor(0, colour);
        ramp.SetColor(1, colour);
        return new GradientTexture2D { Gradient = ramp, Width = wide, Height = tall };
    }
}
