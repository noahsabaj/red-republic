using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// Who works where, on what roster, and who goes short when there are not enough
/// people.
/// </summary>
/// <remarks>
/// <para>
/// <b>Standing is the plan's central decision.</b> The labour pass fills
/// workplaces in standing order, so this is what says the mine is manned before
/// the offices are — the difference between a republic that eats and one that
/// does not. The table's own figure is the opening position; this is how the
/// player disagrees with it.
/// </para>
/// <para>
/// <b>The roster is what night is made of.</b> One crew is a day shift and is
/// what every authored output rate describes; three is round the clock, three
/// times the goods and three times the people — who have to be housed, fed and
/// carried to work in the dark. Zero mothballs the place.
/// </para>
/// </remarks>
public sealed partial class LabourScreen : Screen
{
    private VBoxContainer _workplaces = null!;
    private VBoxContainer _rules = null!;
    private VBoxContainer _people = null!;
    private Label _hours = null!;

    protected override string Title => "Labour";

    protected override string Blurb =>
        "Workplaces are filled in standing order. Whatever is last goes unmanned "
        + "when the republic is short of people.";

    public override void Refresh()
    {
        Rules();

        Workplaces();
        People();
        _hours.Text = $"{Republic.Buildings.Shifts.National:F1} h";
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);

        var national = Parts.Row(body);
        national.AddChild(Parts.Cell(Parts.Say("The national working day"), 2.0f));
        _hours = Parts.Figure("—");
        national.AddChild(Parts.Cell(_hours, 0.6f, HorizontalAlignment.Right));
        national.AddChild(Parts.Cell(Parts.Stepper(Hours, 0.5), 0.8f));

        var columns = Parts.Columns(body);
        var work = Parts.Section(columns, "Workplaces", "Standing, roster and who turned up.", 2.0f);
        _rules = new VBoxContainer();
        work.AddChild(_rules);
        _workplaces = Parts.Scroller(work);
        _people = Parts.Scroller(
            Parts.Section(columns, "The population", "By stage of life and by schooling.", 1.0f));
    }

    private void Hours(double by)
    {
        var asked = Republic.Buildings.Shifts.National + by;
        // A refusal carries a sentence the simulation wrote, and it goes where
        // the player is already looking rather than into a tooltip they would
        // have to go hunting for.
        Ask(Command.SetNationalShiftHours(asked));
        Refresh();
    }

    private void Workplaces()
    {
        Clear(_workplaces);
        Parts.Head(
            _workplaces,
            new Column("Workplace", 2.0f, HorizontalAlignment.Left),
            new Column("Standing", 1.0f, HorizontalAlignment.Left),
            new Column("Staff", 0.8f, HorizontalAlignment.Right),
            new Column("Crews", 0.9f, HorizontalAlignment.Right),
            new Column("", 0.9f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        var any = false;

        foreach (var b in Republic.Buildings.ByStanding())
        {
            var kind = Republic.Buildings.KindAt(b);
            if (!Republic.Buildings.IsBuilt(b) || t.BWorkers[kind] == 0)
            {
                continue;
            }

            any = true;
            var id = Republic.Buildings.IdAt(b);
            var line = Parts.Row(_workplaces, b % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say(t.BName[kind]), 2.0f));

            var standing = Parts.Press(
                $"{Republic.Buildings.PriorityAt(b)}", "Quiet");
            standing.Pressed += () => Cycle(id, Republic.Buildings.PriorityAt(b));
            line.AddChild(Parts.Cell(standing, 1.0f));

            var staff = Republic.Buildings.StaffAt(b);
            var wanted = Republic.Buildings.Jobs(b);
            var count = Parts.Figure($"{staff}/{wanted}");

            // Tinted by what the simulation says: a workplace nobody reached is
            // the thing the player can act on, and the palette is where the
            // colour comes from.
            count.AddThemeColorOverride(
                "font_color", staff < wanted ? Palette.Alarm : Palette.Paper);
            line.AddChild(Parts.Cell(count, 0.8f, HorizontalAlignment.Right));

            line.AddChild(Parts.Cell(
                Parts.Figure($"{Republic.Buildings.ShiftsAt(b)}"), 0.9f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Stepper(by => Shifts(id, (int)by)), 0.9f));
        }

        if (!any)
        {
            _workplaces.AddChild(Parts.Prose("The republic has no workplaces yet.", "Faint"));
        }
    }

    /// <summary>
    /// Exceptions to the national working day, by kind of workplace.
    /// </summary>
    /// <remarks>
    /// <i>"Doctors work twelve."</i> A rule about a kind covers every such
    /// building including ones not yet built, which is the difference between a
    /// policy and a batch edit — and it is the difference a planned economy is
    /// about. The verb existed and nothing offered it, so the only roster a
    /// player could set was the whole republic's or one building at a time.
    /// </remarks>
    private void Rules()
    {
        Clear(_rules);
        var t = Republic.Tables;

        _rules.AddChild(Parts.Say("RULES BY KIND", "Section"));
        _rules.AddChild(Parts.Prose(
            "An exception to the national day, for every workplace of a kind — "
            + "including the ones not built yet.", "Faint"));

        var standing = Republic.Buildings.Shifts.KindRules;
        foreach (var (kind, hours) in standing)
        {
            var line = Parts.Row(_rules);
            var at = kind;
            line.AddChild(Parts.Cell(Parts.Say(t.BName[kind]), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{hours:F1} h"), 0.8f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Stepper(by => Ask(Command.SetKindHours(at, Hours(at) + by)), 0.5), 0.9f));

            var clear = Parts.Press("Clear", "Quiet");
            clear.Pressed += () =>
            {
                Ask(Command.SetKindHours(at, null));
                Refresh();
            };

            line.AddChild(Parts.Cell(clear, 0.9f));
        }

        // Somewhere to start one. The first kind with a workplace standing and
        // no rule of its own — a control the player uses once per rule rather
        // than a list of sixty-four.
        var next = Parts.Press("Set a rule for a kind", "Quiet");
        next.Pressed += () =>
        {
            for (var b = 0; b < Republic.Buildings.Count; b++)
            {
                var kind = Republic.Buildings.KindAt(b);
                if (t.BWorkers[kind] > 0 && Republic.Buildings.Shifts.OfKind(kind) is null)
                {
                    Ask(Command.SetKindHours(kind, Republic.Buildings.Shifts.National));
                    Refresh();
                    return;
                }
            }
        };

        _rules.AddChild(next);
        _rules.AddChild(Parts.Gap(Palette.GapWide));
    }

    private double Hours(int kind) =>
        Republic.Buildings.Shifts.OfKind(kind) ?? Republic.Buildings.Shifts.National;

    private void Cycle(int building, Priority now)
    {
        var next = now switch
        {
            Priority.First => Priority.Ordinary,
            Priority.Ordinary => Priority.Last,
            _ => Priority.First,
        };

        Ask(Command.SetPriority(building, next));
        Refresh();
    }

    private void Shifts(int building, int by)
    {
        var i = Republic.Buildings.IndexOf(building);
        if (i < 0)
        {
            return;
        }

        Ask(Command.SetShifts(building, Republic.Buildings.ShiftsAt(i) + by));
        Refresh();
    }

    private void People()
    {
        Clear(_people);
        Parts.Head(
            _people,
            new Column("Who", 2.0f, HorizontalAlignment.Left),
            new Column("Count", 1.0f, HorizontalAlignment.Right));

        var at = 0;
        foreach (var stage in Enum.GetValues<LifeStage>())
        {
            var line = Parts.Row(_people, at++ % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say($"{stage}"), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{Republic.Citizens.ByStage(stage)}"),
                1.0f, HorizontalAlignment.Right));
        }

        foreach (var taught in Enum.GetValues<Education>())
        {
            var line = Parts.Row(_people, at++ % 2 == 1);
            line.AddChild(Parts.Cell(Parts.Say($"{taught}"), 2.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{Republic.Citizens.ByEducation(taught)}"),
                1.0f, HorizontalAlignment.Right));
        }

        var waiting = Parts.Row(_people, at % 2 == 1);
        waiting.AddChild(Parts.Cell(Parts.Say("At the frontier"), 2.0f));
        waiting.AddChild(Parts.Cell(
            Parts.Figure($"{Republic.Migration.HeadsWaiting()}"),
            1.0f, HorizontalAlignment.Right));
    }
}
