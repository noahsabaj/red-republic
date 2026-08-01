using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// What the republic can put up, what it costs, and what is going up now.
/// </summary>
/// <remarks>
/// <para>
/// <b>Everything is sourced inside the republic and costs labour and materials
/// and no money at all.</b> There is no instant build here and there will not be
/// one — paying to make a problem vanish is the shape this game exists to
/// refuse. The one thing money buys is a <i>contractor</i>, who brings their own
/// hands and their own materials and charges several times what your own crews
/// do, which is the whole argument for a Construction Office.
/// </para>
/// <para>
/// The bill is read off the table rather than restated here. A screen that
/// carried its own copy of what a brickworks costs would be a second version of
/// the balance, and only one of them is tested.
/// </para>
/// </remarks>
public sealed partial class BuildScreen : Screen
{
    private VBoxContainer _catalogue = null!;
    private VBoxContainer _going = null!;
    private VBoxContainer _ways = null!;
    private VBoxContainer _orders = null!;
    private Button _contract = null!;
    private Button _lamps = null!;

    /// <summary>What the player has chosen to place, or -1.</summary>
    public int Chosen { get; private set; } = -1;

    /// <summary>
    /// Which bloc's firm the next building goes to, or null for the republic's
    /// own crews.
    /// </summary>
    /// <remarks>
    /// <b>The opening move, and it had no control at all.</b> A blank map has no
    /// Construction Office, no crews and no materials, so an ordinary site is
    /// one nothing can ever work — the founding's own remarks say the republic
    /// starts by contracting a Bloc firm, and the player could not.
    /// </remarks>
    public Market? Contractor { get; private set; }

    /// <summary>Raised when the player picks a building, so the world can site it.</summary>
    public event Action<int, Market?>? Picked;

    /// <summary>Raised when the player picks a way or a line to draw.</summary>
    public event Action<Cursor, int, bool>? Drawing;

    protected override string Title => "Construction";

    protected override string Blurb =>
        "Nothing here is bought finished. A site is ordered, materialled and "
        + "worked by a crew — or contracted to a Bloc firm, at several times the price.";

    public override void Refresh()
    {
        Catalogue();
        Going();
        Ways();
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);
        var columns = Parts.Columns(body);

        _catalogue = Parts.Scroller(
            Parts.Section(columns, "What may be built", "Cost in materials and builder-days.", 1.4f));

        var right = Parts.Section(columns, "Under way", "Sites, runs and spans the crews are on.", 1.0f);
        _going = Parts.Scroller(right);
        _ways = new VBoxContainer();
        right.AddChild(_ways);

        _orders = new VBoxContainer();
        right.AddChild(_orders);
        Orders();
    }

    /// <summary>
    /// Who builds it, and what else can be ordered.
    /// </summary>
    /// <remarks>
    /// Built once rather than refreshed: the grades and the utilities are a
    /// table, not a republic, so nothing here changes while the game runs.
    /// </remarks>
    private void Orders()
    {
        var t = Republic.Tables;

        _orders.AddChild(Parts.Gap(Palette.GapWide));
        _orders.AddChild(Parts.Say("WHO BUILDS IT", "Section"));
        _orders.AddChild(Parts.Prose(
            "Your own crews cost labour and materials and no money. A Bloc firm "
            + "brings both and charges several times what a builder-day costs.",
            "Faint"));

        var who = Parts.Row(_orders);
        _contract = Parts.Toggle();
        _contract.Toggled += on =>
        {
            Contractor = on ? Market.East : null;
            _contract.Text = on ? "EAST" : "OUR CREWS";
        };

        _contract.Text = "OUR CREWS";
        who.AddChild(Parts.Cell(Parts.Say("contracted to"), 1.6f));
        who.AddChild(Parts.Cell(_contract, 1.0f));

        _orders.AddChild(Parts.Gap(Palette.GapWide));
        _orders.AddChild(Parts.Say("ORDER A WAY", "Section"));
        _orders.AddChild(Parts.Prose(
            "Two clicks: where it starts and where it ends. Nothing routes over "
            + "it until the last builder-day lands.", "Faint"));

        var lit = Parts.Row(_orders);
        _lamps = Parts.Toggle();
        _lamps.Toggled += on => _lamps.Text = Parts.Word(on);
        lit.AddChild(Parts.Cell(Parts.Say("street lighting"), 1.6f));
        lit.AddChild(Parts.Cell(_lamps, 1.0f));

        for (var grade = 0; grade < t.Grades.Length; grade++)
        {
            var line = Parts.Row(_orders, grade % 2 == 1);
            var at = grade;
            var pick = Parts.Press(t.Grades[grade].Name, "Quiet");
            pick.Pressed += () => Drawing?.Invoke(Cursor.Way, at, _lamps.ButtonPressed);

            line.AddChild(Parts.Cell(pick, 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{t.Grades[grade].SpeedKph:F0} km/h"), 1.0f, HorizontalAlignment.Right));
        }

        _orders.AddChild(Parts.Gap(Palette.GapWide));
        _orders.AddChild(Parts.Say("STRING A LINE", "Section"));

        for (var utility = 0; utility < t.Utilities.Length; utility++)
        {
            var line = Parts.Row(_orders, utility % 2 == 1);
            var at = utility;
            var pick = Parts.Press(t.Utilities[utility].Name, "Quiet");
            pick.Pressed += () => Drawing?.Invoke(Cursor.Span, at, false);

            line.AddChild(Parts.Cell(pick, 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{t.Utilities[utility].Reach:F0} m"), 1.0f, HorizontalAlignment.Right));
        }
    }

    private void Catalogue()
    {
        Clear(_catalogue);
        Parts.Head(
            _catalogue,
            new Column("Building", 2.2f, HorizontalAlignment.Left),
            new Column("Workers", 0.7f, HorizontalAlignment.Right),
            new Column("Days", 0.7f, HorizontalAlignment.Right),
            new Column("Materials", 2.0f, HorizontalAlignment.Left));

        var t = Republic.Tables;
        for (var kind = 0; kind < t.BuildingCount; kind++)
        {
            var line = Parts.Row(_catalogue, kind % 2 == 1);
            var pick = Parts.Press(t.BName[kind], kind == Chosen ? "Primary" : "Quiet");

            var at = kind;
            pick.Pressed += () =>
            {
                Chosen = at;
                Picked?.Invoke(at, Contractor);
                Refresh();
            };

            line.AddChild(Parts.Cell(pick, 2.2f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{t.BWorkers[kind]}"), 0.7f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure(Parts.Clean(t.BLabour[kind])), 0.7f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(Parts.Say(Bill(kind), "Small"), 2.0f));
        }
    }

    /// <summary>What a kind is made of, read off the table.</summary>
    private string Bill(int kind)
    {
        var t = Republic.Tables;
        var res = t.Materials.KeysOf(kind);
        var qty = t.Materials.ValuesOf(kind);
        if (res.Length == 0)
        {
            return "nothing but labour";
        }

        var text = new System.Text.StringBuilder();
        for (var i = 0; i < res.Length; i++)
        {
            if (i > 0)
            {
                text.Append(" · ");
            }

            text.Append(Parts.Clean(qty[i])).Append(" t ").Append(t.ResourceNames[res[i]]);
        }

        return text.ToString();
    }

    private void Going()
    {
        Clear(_going);
        Parts.Head(
            _going,
            new Column("Site", 2.0f, HorizontalAlignment.Left),
            new Column("Done", 0.8f, HorizontalAlignment.Right),
            new Column("Worked by", 1.2f, HorizontalAlignment.Left));

        var t = Republic.Tables;
        var any = false;
        for (var b = 0; b < Republic.Buildings.Count; b++)
        {
            if (Republic.Buildings.IsBuilt(b))
            {
                continue;
            }

            any = true;
            var line = Parts.Row(_going, b % 2 == 1);
            line.AddChild(Parts.Cell(
                Parts.Say(t.BName[Republic.Buildings.KindAt(b)]), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{Republic.Buildings.Progress(b) * 100.0:F0}%"),
                0.8f, HorizontalAlignment.Right));

            var contracted = Republic.Buildings.ContractorAt(b) >= 0;
            var crew = Republic.Crews.WorkingAt(
                Destination.Building(Republic.Buildings.IdAt(b)));

            line.AddChild(Parts.Cell(
                Parts.Say(
                    contracted ? "a Bloc firm" : crew is null ? "nobody yet" : $"{crew.Heads} builders",
                    contracted || crew is not null ? "Small" : "Alarm"),
                1.2f));
        }

        if (!any)
        {
            _going.AddChild(Parts.Prose("Nothing is going up.", "Faint"));
        }
    }

    private void Ways()
    {
        Clear(_ways);
        _ways.AddChild(Parts.Gap(Palette.GapWide));
        _ways.AddChild(Parts.Say("WAYS AND LINES", "Section"));

        var t = Republic.Tables;
        foreach (var run in Republic.RoadWorks.Sites)
        {
            var line = Parts.Row(_ways);
            line.AddChild(Parts.Cell(
                Parts.Say($"{t.Grades[run.Grade].Name}, {run.Kilometres:F2} km"), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{run.Progress(t) * 100.0:F0}%"), 0.8f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(CallOff(Destination.RoadSite(run.Id)), 0.9f));
        }

        foreach (var span in Republic.LineWorks.Sites)
        {
            var line = Parts.Row(_ways);
            line.AddChild(Parts.Cell(
                Parts.Say($"{t.Utilities[span.Kind].Name}, {span.Kilometres:F2} km"), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{span.Progress(t) * 100.0:F0}%"), 0.8f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(CallOff(Destination.LineSite(span.Id)), 0.9f));
        }

        if (Republic.RoadWorks.Sites.Count + Republic.LineWorks.Sites.Count == 0)
        {
            _ways.AddChild(Parts.Prose("No way or line is being laid.", "Faint"));
        }
    }

    /// <summary>
    /// The button that calls a run off.
    /// </summary>
    /// <remarks>
    /// Not gated behind a confirmation. What the republic has already delivered
    /// to the site is lost with it, which is a consequence rather than a trap —
    /// and the refusal, if a gang is on it, arrives in the simulation's own
    /// words.
    /// </remarks>
    private Button CallOff(Destination site)
    {
        var button = Parts.Press("Call off", "Quiet");
        button.Pressed += () =>
        {
            Ask(Command.CancelWorks(site));
            Refresh();
        };

        return button;
    }
}
