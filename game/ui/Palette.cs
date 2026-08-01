using Godot;

namespace RedRepublic.Ui;

/// <summary>
/// Every colour and every measurement the interface is made of, in one file.
/// </summary>
/// <remarks>
/// <para>
/// <b>This is the only place a colour is chosen.</b> <c>theme.tres</c> is
/// generated from it, and the handful of places that must tint a control from
/// code — a tonnage that has run out, a component that is failing — read the
/// constants here rather than typing a triple. A second copy of a chosen colour
/// is the copy that drifts, and this interface had five of them.
/// </para>
/// <para>
/// Presentation only. Nothing in this file may change how a republic turns out.
/// </para>
/// <para>
/// <b>The theme is "a form issued by a ministry".</b> The player is an
/// administrator reading returns and signing orders, and the screen is the
/// paperwork. That is why there are no rounded corners, no icons, no gradients
/// and no shadows — a printed form has none of those. What it does have is a
/// stamped title block, a rule under it, ruled tables with the figures in a
/// monospace, and exactly one colour that means "this is the decision".
/// </para>
/// <para>
/// The three inks it is printed in: <b>red</b> for a decision or a refusal and
/// nothing else; <b>ochre</b> for headings inside a table, so a section reads as
/// a section without a second size of type; <b>paper white</b> for everything a
/// player reads. Alarm and good are deliberately close to red and to nothing:
/// this game says which thing is worst rather than scoring you, so the palette
/// has one shout in it and no traffic light.
/// </para>
/// </remarks>
public static class Palette
{
    // ---- ink: the grounds, darkest to lightest ----

    /// <summary>The deepest ground: a full-screen modal, and the backdrop behind a menu.</summary>
    public static Color Ink { get; } = new(0.051f, 0.059f, 0.063f);

    /// <summary>A panel: the sheet of paper a section is printed on.</summary>
    public static Color Carbon { get; } = new(0.082f, 0.094f, 0.102f);

    /// <summary>A row, a card, a control at rest — one step up off the sheet.</summary>
    public static Color CarbonRaised { get; } = new(0.110f, 0.125f, 0.137f);

    /// <summary>A well: an input, a pressed button, a scroll track.</summary>
    public static Color CarbonSunk { get; } = new(0.039f, 0.047f, 0.051f);

    /// <summary>The hairline every box and every table row is ruled with.</summary>
    public static Color Rule { get; } = new(0.180f, 0.204f, 0.220f);

    /// <summary>
    /// A rule that has to be seen from across the screen: a focused control, the
    /// division between two halves of a screen.
    /// </summary>
    public static Color RuleStrong { get; } = new(0.271f, 0.302f, 0.322f);

    // ---- the inks it is printed in ----

    /// <summary>
    /// Anything a player reads. Warm rather than blue-white: this is newsprint
    /// under a desk lamp, not a terminal.
    /// </summary>
    public static Color Paper { get; } = new(0.894f, 0.882f, 0.855f);

    /// <summary>A label beside a figure, a sentence of explanation.</summary>
    public static Color PaperDim { get; } = new(0.659f, 0.651f, 0.627f);

    /// <summary>A unit, a caption, a thing present and not being read.</summary>
    public static Color PaperFaint { get; } = new(0.435f, 0.443f, 0.451f);

    /// <summary>Institutional red: the one action a screen exists for, and a refusal.</summary>
    public static Color Red { get; } = new(0.702f, 0.216f, 0.169f);

    public static Color RedHot { get; } = new(0.824f, 0.271f, 0.212f);

    /// <summary>A heading inside a table, and a stamp.</summary>
    public static Color Ochre { get; } = new(0.769f, 0.635f, 0.290f);

    /// <summary>
    /// What the republic paints its lorries. One shade for the whole fleet:
    /// telling a coach from a heavy lorry is the silhouette's job, and it is
    /// sized off the table for exactly that reason.
    /// </summary>
    public static Color Vehicle { get; } = new(0.427f, 0.451f, 0.400f);

    /// <summary>Something is wrong and the player can act on it.</summary>
    public static Color Alarm { get; } = new(0.886f, 0.463f, 0.361f);

    /// <summary>Something is being done well. Used sparingly.</summary>
    public static Color Good { get; } = new(0.498f, 0.663f, 0.420f);

    // ---- measurement ----
    //
    // Everything is a multiple of four, and most of it a multiple of eight. A
    // form is ruled paper; nothing on it sits at an arbitrary offset.

    public const int Step = 4;
    public const int GapTight = 4;
    public const int Gap = 8;
    public const int GapWide = 16;
    public const int GapSection = 24;

    /// <summary>The margin inside a panel, and the margin round a full screen.</summary>
    public const int Pad = 16;

    public const int MarginScreen = 48;
    public const int MarginScreenTop = 40;

    /// <summary>
    /// A row in a table. Tall enough for a 15 px face with a rule under it and no
    /// more.
    /// </summary>
    public const int RowHeight = 30;

    public const int ButtonHeight = 34;

    /// <summary>A <c>-</c> or <c>+</c> beside a number.</summary>
    public const int StepButton = 28;

    // ---- type ----
    //
    // Three faces, each with one job.

    public const string FaceText = "res://art/fonts/PTSans-Regular.ttf";
    public const string FaceTextBold = "res://art/fonts/PTSans-Bold.ttf";
    public const string FaceTextItalic = "res://art/fonts/PTSans-Italic.ttf";
    public const string FaceNarrow = "res://art/fonts/PTSansNarrow-Regular.ttf";
    public const string FaceNarrowBold = "res://art/fonts/PTSansNarrow-Bold.ttf";
    public const string FaceFigure = "res://art/fonts/PTMono-Regular.ttf";

    public const int SizeTitle = 38;
    public const int SizeSection = 19;
    public const int SizeLabel = 14;
    public const int SizeBody = 15;
    public const int SizeSmall = 13;
    public const int SizeFigure = 14;
    public const int SizeFigureBig = 21;
    public const int SizeTiny = 11;
}
