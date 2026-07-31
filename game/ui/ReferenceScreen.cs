using System;
using Godot;

namespace RedRepublic.Ui;

/// <summary>
/// The tables, as the player can read them.
/// </summary>
/// <remarks>
/// <para>
/// <b>A game that explains itself is a clause of the release standard</b>, and
/// this is where it is met: every authored figure the player could otherwise
/// only learn by losing a republic to it.
/// </para>
/// <para>
/// Every figure is read from the balance table. Nothing here is restated, so a
/// change to what a lorry carries changes this page in the same commit and
/// cannot be forgotten.
/// </para>
/// </remarks>
public sealed partial class ReferenceScreen : Screen
{
    private VBoxContainer _goods = null!;
    private VBoxContainer _ways = null!;
    private VBoxContainer _fleet = null!;

    protected override string Title => "Reference";

    protected override string Blurb =>
        "What everything costs, carries and is worth. Read from the same table "
        + "the republic runs on.";

    public override void Refresh()
    {
        Goods();
        Ways();
        Fleet();
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);
        var columns = Parts.Columns(body);

        _goods = Parts.Scroller(
            Parts.Section(columns, "Goods", "What each is worth to a bloc, per tonne.", 1.2f));

        var right = Parts.Section(columns, "Ways, lines and vehicles", "", 1.4f);
        _ways = new VBoxContainer();
        right.AddChild(_ways);
        _fleet = Parts.Scroller(right);
    }

    private void Goods()
    {
        Clear(_goods);
        Parts.Head(
            _goods,
            new Column("Goods", 1.6f, HorizontalAlignment.Left),
            new Column("Roubles", 0.9f, HorizontalAlignment.Right),
            new Column("Dollars", 0.9f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        for (var r = 0; r < t.Resources.Length; r++)
        {
            var line = Parts.Row(_goods, r % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say(t.ResourceNames[r]), 1.6f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{t.ResourcePriceEast[r]:F0}"), 0.9f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{t.ResourcePriceWest[r]:F0}"), 0.9f, HorizontalAlignment.Right));
        }
    }

    private void Ways()
    {
        Clear(_ways);
        _ways.AddChild(Parts.Say("WAYS", "Section"));
        Parts.Head(
            _ways,
            new Column("Grade", 1.4f, HorizontalAlignment.Left),
            new Column("Carries", 0.9f, HorizontalAlignment.Left),
            new Column("Speed", 0.8f, HorizontalAlignment.Right),
            new Column("Days/km", 0.8f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        for (var g = 0; g < t.Grades.Length; g++)
        {
            var grade = t.Grades[g];
            var line = Parts.Row(_ways, g % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say(grade.Name), 1.4f));
            line.AddChild(Parts.Cell(Parts.Say($"{grade.Carries}", "Small"), 0.9f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{grade.SpeedKph:F0} km/h"), 0.8f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure(Parts.Clean(grade.Labour)), 0.8f, HorizontalAlignment.Right));
        }

        _ways.AddChild(Parts.Gap(Palette.GapWide));
        _ways.AddChild(Parts.Say("LINES", "Section"));
        Parts.Head(
            _ways,
            new Column("Line", 1.4f, HorizontalAlignment.Left),
            new Column("Reach", 0.8f, HorizontalAlignment.Right),
            new Column("Loss/km", 0.8f, HorizontalAlignment.Right),
            new Column("Carries", 1.2f, HorizontalAlignment.Left));

        for (var u = 0; u < t.Utilities.Length; u++)
        {
            var utility = t.Utilities[u];
            var line = Parts.Row(_ways, u % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say(utility.Name), 1.4f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{utility.Reach:F0} m"), 0.8f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{utility.LossPerKm * 100.0:F1}%"), 0.8f, HorizontalAlignment.Right));

            var carries = utility.Carries.Length == 0
                ? "current or heat"
                : string.Join(", ", Array.ConvertAll(utility.Carries, r => t.ResourceNames[r]));
            line.AddChild(Parts.Cell(Parts.Say(carries, "Small"), 1.2f));
        }
    }

    private void Fleet()
    {
        Clear(_fleet);
        _fleet.AddChild(Parts.Gap(Palette.GapWide));
        _fleet.AddChild(Parts.Say("VEHICLES", "Section"));
        Parts.Head(
            _fleet,
            new Column("Vehicle", 1.6f, HorizontalAlignment.Left),
            new Column("Role", 1.0f, HorizontalAlignment.Left),
            new Column("Carries", 0.9f, HorizontalAlignment.Right),
            new Column("Seats", 0.7f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        for (var v = 0; v < t.VehicleIds.Length; v++)
        {
            var line = Parts.Row(_fleet, v % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say(t.VName[v]), 1.6f));
            line.AddChild(Parts.Cell(Parts.Say(t.VRole[v], "Small"), 1.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure(t.VCapacity[v] <= 0.0 ? "—" : $"{t.VCapacity[v]:F0} t"),
                0.9f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure(t.VSeats[v] == 0 ? "—" : $"{t.VSeats[v]}"),
                0.7f, HorizontalAlignment.Right));
        }
    }
}
