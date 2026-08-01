using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// Everything the player has actually done, in order.
/// </summary>
/// <remarks>
/// <para>
/// <b>A save records how its republic came to be</b>, which is what makes a
/// replay possible at all — and this is that record, read back. Refused commands
/// are not here, and that is not an omission: a refusal changes nothing, so
/// replaying it would be replaying a no-op.
/// </para>
/// <para>
/// It reads the journal rather than keeping its own log. A second history would
/// be a second answer to "what happened", and only one of them travels in the
/// save.
/// </para>
/// </remarks>
public sealed partial class JournalScreen : Screen
{
    private VBoxContainer _entries = null!;

    protected override string Title => "Journal";

    protected override string Blurb =>
        "Every accepted order, in the order it was given. What a save carries, so "
        + "a republic can be replayed from how it came to be.";

    public override void Refresh()
    {
        Clear(_entries);
        Parts.Head(
            _entries,
            new Column("Day", 0.6f, HorizontalAlignment.Right),
            new Column("Order", 3.0f, HorizontalAlignment.Left));

        var journal = Republic.Journal.Entries;
        var from = Math.Max(0, journal.Count - 400);

        for (var i = journal.Count - 1; i >= from; i--)
        {
            var (tick, command) = journal[i];
            var line = Parts.Row(_entries, i % 2 == 1);
            var date = Date.FromDayIndex(tick / SimClock.TicksPerDay);

            line.AddChild(Parts.Cell(
                Parts.Figure($"{date.Year:0000}-{date.Month:00}-{date.Day:00}"),
                0.6f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(Parts.Say(Sentence(command)), 3.0f));
        }

        if (journal.Count == 0)
        {
            _entries.AddChild(Parts.Prose("Nothing has been ordered yet.", "Faint"));
        }
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);
        _entries = Parts.Scroller(body);
    }

    /// <summary>
    /// One order, in words.
    /// </summary>
    /// <remarks>
    /// <b>The sentence is written here and the names come from the table.</b> A
    /// command carries indices, because the simulation has no opinion about
    /// English; turning them into something a person reads is the interface's
    /// job, and the names it uses are the ones authored beside the things they
    /// name.
    /// </remarks>
    private string Sentence(Command c)
    {
        var t = Republic.Tables;
        return c.Kind switch
        {
            CommandKind.Place => $"commissioned a {Kind(c.A)}",
            CommandKind.ContractBuild => $"contracted a {Kind(c.A)} to the {Words.Of((Market)c.B)}",
            CommandKind.Demolish => "pulled a building down",
            CommandKind.CancelWorks => "called a run off",
            CommandKind.OrderRoad =>
                $"ordered {Length(c)} of {t.Grades[Math.Clamp(c.A, 0, t.Grades.Length - 1)].Name}",
            CommandKind.OrderLine =>
                $"ordered {Length(c)} of {t.Utilities[Math.Clamp(c.A, 0, t.Utilities.Length - 1)].Name}",
            CommandKind.RecallCrew => "called a crew off a site",
            CommandKind.SetImportPolicy => "set where materials are bought in",
            CommandKind.ClearImportPolicy => "put a site back on the republic's policy",
            CommandKind.SetStandingOrder =>
                $"told a store to keep {Parts.Clean(c.Amount)} t of {Goods(c.B)}",
            CommandKind.HireForeign => $"hired {c.C} builders from the {Words.Of((Market)c.A)}",
            CommandKind.AcceptContract => "accepted a tender",
            CommandKind.DeclineContract => "declined a tender",
            CommandKind.AddTradeRule =>
                $"told the customs houses to {Words.Of((TradeAction)c.C)} {Goods(c.A)} "
                + $"with the {Words.Of((Market)c.B)}",
            CommandKind.RemoveTradeRule => "withdrew a trade rule",
            CommandKind.MoveTradeRule => "changed the order the trade rules are served in",
            CommandKind.TakeLoan => $"took an advance from the {Words.Of((Market)c.A)}",
            CommandKind.RepayLoan => $"repaid the {Words.Of((Market)c.A)}",
            CommandKind.SetNationalShiftHours =>
                $"set the working day to {Parts.Clean(c.Amount)} hours",
            CommandKind.SetShiftHours => "set an exception to the working day",
            CommandKind.SetShifts => $"put a workplace on {c.B} crews",
            CommandKind.SetPriority => $"set a workplace's standing to {Words.Of((Priority)c.B)}",
            CommandKind.NameRepublic => $"named the republic {c.Text}",
            // Not a fallback anybody should ever read. A verb with no sentence
            // here is a page of a republic's history that says nothing, and
            // `ExposureTests` fails the build rather than letting it ship —
            // this line is what a player sees if that guard is ever removed.
            _ => "did something the journal cannot name",
        };
    }

    private string Kind(int kind) =>
        kind >= 0 && kind < Republic.Tables.BuildingCount
            ? Republic.Tables.BName[kind]
            : "building";

    private string Goods(int resource) =>
        resource >= 0 && resource < Republic.Tables.Resources.Length
            ? Republic.Tables.ResourceNames[resource]
            : "goods";

    private static string Length(Command c) =>
        $"{Units.Distance(c.X, c.Y, c.ToX, c.ToY) / 1000.0:F2} km";
}
