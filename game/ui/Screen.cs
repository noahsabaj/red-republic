using System;
using Godot;

namespace RedRepublic.Ui;

/// <summary>
/// One full screen of the interface: a title block, a rule under it, and a body.
/// </summary>
/// <remarks>
/// <para>
/// <b>The title block is the one piece of chrome every screen shares.</b> A
/// stamped name, a line saying what the screen is for, and a red rule — which is
/// what makes nine screens read as nine pages of the same document rather than
/// as nine applications.
/// </para>
/// <para>
/// A screen builds its body once and refreshes it. Building a node per datum per
/// frame is what makes a list of five thousand cost a visible hitch, where
/// updating the same labels costs nothing measurable.
/// </para>
/// </remarks>
public abstract partial class Screen : Control
{
    private VBoxContainer _body = null!;
    private bool _raised;

    /// <summary>What the screen is called, stamped at the top.</summary>
    protected abstract string Title { get; }

    /// <summary>One line saying what it is for. Empty for a screen that needs none.</summary>
    protected virtual string Blurb => "";

    /// <summary>The republic this screen reads. Never written except through a command.</summary>
    protected Sim.World Republic { get; private set; } = null!;

    /// <summary>
    /// Say what the republic refused, or clear it with an empty string.
    /// </summary>
    /// <remarks>
    /// <b>A refusal is a sentence the simulation wrote, and throwing it away is
    /// the one thing a screen must not do with it.</b> Seven of the ten places
    /// that issued a command discarded what came back — the trade screen, the
    /// roster steppers, the finance screen — so the screens most likely to be
    /// told no were the ones that said nothing, which restores exactly the
    /// mystery the architecture rule exists to prevent.
    /// </remarks>
    public event Action<string>? Refused;

    /// <summary>
    /// Ask the republic for something, and print what it says if it says no.
    /// </summary>
    /// <remarks>
    /// The one path a screen issues through, so a discarded refusal is a thing
    /// somebody has to go out of their way to write.
    /// </remarks>
    protected Sim.Outcome Ask(Sim.Command command)
    {
        var outcome = Republic.Issue(command);
        Refused?.Invoke(outcome.Accepted ? "" : outcome.Refusal);
        return outcome;
    }

    /// <summary>Open the screen over a republic, building it the first time.</summary>
    public void Open(Sim.World republic)
    {
        Republic = republic;
        Visible = true;

        if (!_raised)
        {
            Raise();
            _raised = true;
        }

        Refresh();
    }

    public void Shut() => Visible = false;

    /// <summary>Bring the figures up to date. Called on opening and every second.</summary>
    public abstract void Refresh();

    public override void _Ready()
    {
        Visible = false;
        SetAnchorsPreset(LayoutPreset.FullRect);
        MouseFilter = MouseFilterEnum.Stop;

        // The deepest ground, so a screen is a sheet of paper on a desk rather
        // than a window floating over the map. A screen is read, not glanced at.
        var backdrop = new ColorRect
        {
            Color = Palette.Ink,
            MouseFilter = MouseFilterEnum.Ignore,
        };

        backdrop.SetAnchorsPreset(LayoutPreset.FullRect);
        AddChild(backdrop);

        var margin = new MarginContainer();
        margin.SetAnchorsPreset(LayoutPreset.FullRect);
        margin.AddThemeConstantOverride("margin_left", Palette.MarginScreen);
        margin.AddThemeConstantOverride("margin_right", Palette.MarginScreen);
        margin.AddThemeConstantOverride("margin_top", Palette.MarginScreenTop);
        margin.AddThemeConstantOverride("margin_bottom", Palette.MarginScreen);
        AddChild(margin);

        var page = new VBoxContainer();
        page.AddThemeConstantOverride("separation", Palette.GapSection);
        margin.AddChild(page);

        var header = new PanelContainer { ThemeTypeVariation = "Header" };
        page.AddChild(header);

        var block = new VBoxContainer();
        block.AddThemeConstantOverride("separation", Palette.GapTight);
        header.AddChild(block);
        block.AddChild(Parts.Say(Title.ToUpperInvariant(), "Title"));
        if (Blurb.Length > 0)
        {
            block.AddChild(Parts.Prose(Blurb, "Faint"));
        }

        _body = new VBoxContainer
        {
            SizeFlagsVertical = SizeFlags.ExpandFill,
            SizeFlagsHorizontal = SizeFlags.ExpandFill,
        };

        _body.AddThemeConstantOverride("separation", Palette.GapWide);
        page.AddChild(_body);
    }

    /// <summary>Build the body. Called once, the first time the screen is opened.</summary>
    protected abstract void Build(VBoxContainer body);

    private void Raise() => Build(_body);

    /// <summary>
    /// Empty a container and its pooled children.
    /// </summary>
    /// <remarks>
    /// For the lists whose length is a property of the republic rather than of a
    /// roster — the buildings standing, the tenders on the table — where pooling
    /// would mean holding rows for a republic that has since demolished them.
    /// </remarks>
    protected static void Clear(Node into)
    {
        ArgumentNullException.ThrowIfNull(into);
        foreach (var child in into.GetChildren())
        {
            into.RemoveChild(child);
            child.QueueFree();
        }
    }
}
